/// A simplified agent implementation used as a reference
/// It makes no attempt to handle context limits, and cannot read resources
use async_trait::async_trait;
use futures::stream::BoxStream;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use super::Agent;
use crate::agents::capabilities::Capabilities;
use crate::agents::system::{SystemConfig, SystemResult};
use crate::message::{Message, ToolRequest};
use crate::providers::base::Provider;
use crate::providers::base::ProviderUsage;
use crate::register_agent;
use crate::token_counter::TokenCounter;
use indoc::indoc;
use mcp_core::tool::Tool;
use serde_json::{json, Value};

/// Reference implementation of an Agent
pub struct ReferenceAgent {
    capabilities: Mutex<Capabilities>,
    _token_counter: TokenCounter,
}

impl ReferenceAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            capabilities: Mutex::new(Capabilities::new(provider)),
            _token_counter: TokenCounter::new(),
        }
    }
}

#[async_trait]
impl Agent for ReferenceAgent {
    async fn add_system(&mut self, system: SystemConfig) -> SystemResult<()> {
        let mut capabilities = self.capabilities.lock().await;
        capabilities.add_system(system).await
    }

    async fn remove_system(&mut self, name: &str) {
        let mut capabilities = self.capabilities.lock().await;
        capabilities
            .remove_system(name)
            .await
            .expect("Failed to remove system");
    }

    async fn list_systems(&self) -> Vec<String> {
        let capabilities = self.capabilities.lock().await;
        capabilities
            .list_systems()
            .await
            .expect("Failed to list systems")
    }

    async fn passthrough(&self, _system: &str, _request: Value) -> SystemResult<Value> {
        // TODO implement
        Ok(Value::Null)
    }

    #[instrument(skip(self, messages), fields(user_message))]
    async fn reply(
        &self,
        messages: &[Message],
    ) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
        let mut messages = messages.to_vec();
        let reply_span = tracing::Span::current();
        let mut capabilities = self.capabilities.lock().await;
        let mut tools = capabilities.get_prefixed_tools().await?;
        // we add in the read_resource tool by default
        // TODO: make sure there is no collision with another system's tool name
        let read_resource_tool = Tool::new(
            "read_resource".to_string(),
            indoc! {r#"
                Read a resource from a system.
        
                Resources allow systems to share data that provide context to LLMs, such as 
                files, database schemas, or application-specific information. This tool searches for the 
                resource URI in the provided system, and reads in the resource content. If no system
                is provided, the tool will search all systems for the resource.
        
                The read_resource tool is typically used with a search query (can be before or after). Here are two examples:
                1. Search for files in Google Drive MCP Server, then call read_resource(gdrive:///<file_id>) to read the Google Drive file.
                2. You need to gather schema information about a Postgres table before creating a query. So you call read_resource(postgres://<host>/<table>/schema)
                    to get the schema information for a table and then use to construct your query.
            "#}.to_string(),
            json!({
                "type": "object",
                "required": ["uri"],
                "properties": {
                    "uri": {"type": "string", "description": "Resource URI"},
                    "system_name": {"type": "string", "description": "Optional system name"}
                }
            }),
        );

        tools.push(read_resource_tool);

        let system_prompt = capabilities.get_system_prompt().await;
        let _estimated_limit = capabilities
            .provider()
            .get_model_config()
            .get_estimated_limit();

        // Set the user_message field in the span instead of creating a new event
        if let Some(content) = messages
            .last()
            .and_then(|msg| msg.content.first())
            .and_then(|c| c.as_text())
        {
            debug!("user_message" = &content);
        }

        // Update conversation history for the start of the reply
        let _resources = capabilities.get_resources().await?;

        Ok(Box::pin(async_stream::try_stream! {
            let _reply_guard = reply_span.enter();
            loop {
                // Get completion from provider
                let (response, usage) = capabilities.provider().complete(
                    &system_prompt,
                    &messages,
                    &tools,
                ).await?;
                capabilities.record_usage(usage).await;

                // Yield the assistant's response
                yield response.clone();

                tokio::task::yield_now().await;

                // First collect any tool requests
                let tool_requests: Vec<&ToolRequest> = response.content
                    .iter()
                    .filter_map(|content| content.as_tool_request())
                    .collect();

                if tool_requests.is_empty() {
                    break;
                }

                // Then dispatch each in parallel
                let futures: Vec<_> = tool_requests
                    .iter()
                    .filter_map(|request| request.tool_call.clone().ok())
                    .map(|tool_call| capabilities.dispatch_tool_call(tool_call))
                    .collect();

                // Process all the futures in parallel but wait until all are finished
                let outputs = futures::future::join_all(futures).await;

                // Create a message with the responses
                let mut message_tool_response = Message::user();
                // Now combine these into MessageContent::ToolResponse using the original ID
                for (request, output) in tool_requests.iter().zip(outputs.into_iter()) {
                    message_tool_response = message_tool_response.with_tool_response(
                        request.id.clone(),
                        output,
                    );
                }

                yield message_tool_response.clone();

                messages.push(response);
                messages.push(message_tool_response);
            }
        }))
    }

    async fn usage(&self) -> Vec<ProviderUsage> {
        let capabilities = self.capabilities.lock().await;
        capabilities.get_usage().await
    }
}

register_agent!("reference", ReferenceAgent);

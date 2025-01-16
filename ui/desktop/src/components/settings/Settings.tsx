import React from 'react';
import { SecretsList } from './SecretsList';

export default function Settings() {
  return (
    <div className="h-screen bg-gray-50 dark:bg-gray-900">
      <div className="max-w-4xl mx-auto">
        <header className="flex items-center p-4 border-b dark:border-gray-800">
          <button 
            onClick={() => window.history.back()}
            className="flex items-center text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white"
          >
            <ArrowLeftIcon className="w-5 h-5 mr-2" />
            Exit
          </button>
        </header>

        <main className="p-6">
          <SecretsList />
        </main>
      </div>
    </div>
  );
}

const ArrowLeftIcon = (props: React.SVGProps<SVGSVGElement>) => (
  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" {...props}>
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
  </svg>
);

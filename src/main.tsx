import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './windows/main/App';
import { enableNativeGlass } from './lib/windowGlass';
import './styles/globals.css';

void enableNativeGlass();

async function start() {
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    // Browser-only preview with sample data (never in a real build).
    const { installMockIpc } = await import('./dev/mockIpc');
    installMockIpc();
  }
  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void start();

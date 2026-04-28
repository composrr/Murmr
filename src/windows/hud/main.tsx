import React from 'react';
import ReactDOM from 'react-dom/client';
import HudApp from './App';
import '../../styles/globals.css';
import './hud.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <HudApp />
  </React.StrictMode>,
);

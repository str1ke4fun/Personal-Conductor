import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { initCustomTheme } from './styles/customTheme';
import { ChatPanel } from './windows/ChatPanel';

initCustomTheme();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <div className="panel-window">
      <ChatPanel standalone />
    </div>
  </React.StrictMode>,
);

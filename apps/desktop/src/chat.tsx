import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { ChatPanel } from './windows/ChatPanel';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <div className="panel-window">
      <ChatPanel standalone />
    </div>
  </React.StrictMode>,
);

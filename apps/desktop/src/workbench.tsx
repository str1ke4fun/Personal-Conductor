import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { AgentWorkspacePanel } from './windows/AgentWorkspacePanel';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AgentWorkspacePanel />
  </React.StrictMode>,
);

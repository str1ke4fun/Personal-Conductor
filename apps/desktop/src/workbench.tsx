import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { initCustomTheme } from './styles/customTheme';
import { AgentWorkspacePanel } from './windows/AgentWorkspacePanel';

initCustomTheme();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AgentWorkspacePanel />
  </React.StrictMode>,
);

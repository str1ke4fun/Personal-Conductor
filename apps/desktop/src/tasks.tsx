import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { initCustomTheme } from './styles/customTheme';
import { TaskPanelContent } from './windows/TaskPanelContent';

initCustomTheme();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <div className="panel-window">
      <TaskPanelContent standalone />
    </div>
  </React.StrictMode>,
);

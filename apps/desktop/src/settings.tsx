import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { initCustomTheme } from './styles/customTheme';
import { SettingsPanel } from './windows/SettingsPanel';

initCustomTheme();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <div className="panel-window">
      <SettingsPanel standalone />
    </div>
  </React.StrictMode>,
);

import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles/app.css';
import { SettingsPanel } from './windows/SettingsPanel';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <div className="panel-window">
      <SettingsPanel standalone />
    </div>
  </React.StrictMode>,
);

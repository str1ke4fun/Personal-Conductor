import ReactDOM from 'react-dom/client';
import { App } from './App';
import './styles/app.css';
import { initCustomTheme } from './styles/customTheme';

initCustomTheme();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<App />);

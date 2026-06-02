import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_DEBUG,
    rollupOptions: {
      input: {
        main: 'index.html',
        tasks: 'tasks.html',
        chat: 'chat.html',
        settings: 'settings.html',
        workbench: 'workbench.html',
      },
    },
  }
});

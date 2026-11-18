import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1430, strictPort: true },
  build: { target: 'es2022', chunkSizeWarningLimit: 2500 },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
} as any);

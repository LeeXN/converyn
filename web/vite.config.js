import { defineConfig } from 'vite';
export default defineConfig({build:{assetsDir:'assets',rollupOptions:{output:{entryFileNames:'assets/app.js',assetFileNames:'assets/[name][extname]'}}}});

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
  // We can add IPC methods here if we need Electron-level features
  // For now, the renderer will talk directly to the Python WebSocket
  serverUrl: 'ws://localhost:8765'
});

const { app, BrowserWindow, shell } = require('electron');
const path = require('path');
const { spawn } = require('child_process');

let mainWindow;
let pythonProcess;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1000,
    height: 800,
    minWidth: 600,
    minHeight: 400,
    frame: false, // Remove standard topbar
    backgroundColor: '#1e1e1e',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
    },
  });

  mainWindow.loadFile('index.html');
  
  // Open links in external browser
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
      shell.openExternal(url);
      return { action: 'deny' };
  });

  mainWindow.webContents.on('will-navigate', (event, url) => {
      if (url !== mainWindow.webContents.getURL()) {
          event.preventDefault();
          shell.openExternal(url);
      }
  });

  mainWindow.on('closed', function () {
    mainWindow = null;
  });
}

function startBackend() {
  const backendDir = path.join(__dirname, '..', 'backend');
  const mainPyPath = path.join(backendDir, 'main.py');
  
  // Detect Python path (use project root venv if it exists)
  const venvPath = path.join(__dirname, '..', 'venv', 'bin', 'python');
  const pythonExe = require('fs').existsSync(venvPath) ? venvPath : 'python3';

  console.log(`Starting backend with: ${pythonExe}`);
  // Use -u for unbuffered binary stdout/stderr
  pythonProcess = spawn(pythonExe, ['-u', mainPyPath]);

  pythonProcess.stdout.on('data', (data) => {
    console.log(`Backend: ${data}`);
  });

  pythonProcess.stderr.on('data', (data) => {
    console.error(`Backend Error: ${data}`);
  });

  pythonProcess.on('close', (code) => {
    console.log(`Backend process exited with code ${code}`);
  });
}

app.whenReady().then(() => {
  startBackend();
  createWindow();

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', function () {
  if (process.platform !== 'darwin') app.quit();
});

app.on('will-quit', () => {
  if (pythonProcess) {
    pythonProcess.kill();
  }
});

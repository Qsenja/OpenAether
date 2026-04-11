const chatHistory = document.getElementById('chat-history');
const userInput = document.getElementById('user-input');
const sendBtn = document.getElementById('send-btn');
const stopBtn = document.getElementById('stop-btn');
const statusText = document.getElementById('status-text');
const logsBtn = document.getElementById('logs-btn');
const logsModal = document.getElementById('logs-modal');
const logsList = document.getElementById('logs-list');
const closeLogsBtn = document.getElementById('close-logs');
const pastebinInput = document.getElementById('pastebin-key');
const logViewerModal = document.getElementById('log-viewer-modal');
const viewerContent = document.getElementById('log-content-area');
const closeViewerBtn = document.getElementById('close-viewer');
const appBody = document.body;
const copyLogBtn = document.getElementById('copy-log-btn');

// Setup Elements
const setupScreen = document.getElementById('setup-screen');
const appContainer = document.getElementById('app');
const startSetupBtn = document.getElementById('start-setup-btn');
const skipSetupBtn = document.getElementById('skip-setup-btn');
const fixDockerBtn = document.getElementById('fix-docker-btn');
const progressWrapper = document.getElementById('setup-progress-wrapper');
const progressBarFill = document.getElementById('setup-progress-fill');
const progressText = document.getElementById('setup-progress-text');
const progressPercent = document.getElementById('setup-progress-percent');

let socket;
let currentTurnContainer = null;
let currentAgentBubble = null;
let currentThoughtContainer = null;
let currentToolGroup = null;
let activeToolCapsules = {}; // call_id -> { capsule, outputDiv }
let currentModelName = 'Aether Core';
let userScrolledUp = false;

// --- SOCKET LOGIC ---
function connect() {
    socket = new WebSocket(window.api.serverUrl);
    
    if (window.marked) {
        marked.setOptions({ breaks: true, gfm: true });
    }

    socket.onopen = () => {
        updateStatusDisplay('idle');
        // Check setup on every connect
        socket.send(JSON.stringify({ type: 'get_setup_status' }));
    };
    socket.onclose = () => {
        updateStatusDisplay('disconnected');
        setTimeout(connect, 2000);
    };

    socket.onmessage = (event) => {
        const data = json_parse(event.data);
        if (!data) return;

        switch(data.type) {
            case 'agent_thought':
            case 'agent_thought_chunk':
                if (appBody.classList.contains('status-idle')) return;
                ensureTurnContainer();
                // If we get thoughts, we are NO LONGER in a tool group
                currentToolGroup = null;
                appendToThought(data.content, data.type === 'agent_thought'); 
                break;
            case 'agent_message':
                if (appBody.classList.contains('status-idle')) return;
                ensureTurnContainer();
                // Messages break both thoughts and tool groups
                currentToolGroup = null;
                currentThoughtContainer = null;
                appendAgentChunk(data.content, data.model);
                break;
            case 'agent_message_done':
                finalizeTurn();
                break;
            case 'tool_call':
                ensureTurnContainer();
                createToolCapsule(data.name, data.args, data.call_id);
                break;
            case 'tool_output':
                updateToolCapsule(data.name, data.output, data.call_id);
                break;
            case 'approval_required':
                renderApprovalCard(data.tool, data.args, data.message);
                break;
            case 'status_update':
                updateStatusDisplay(data.status, data.model);
                break;
            case 'error':
                appendSimpleMessage('system', `ERROR: ${data.content}`);
                break;
            case 'logs_data':
                showLogsModal(data.logs, data.settings);
                break;
            case 'pastebin_result':
                handlePastebinResult(data.result, data.path);
                break;
            case 'log_content':
                viewerContent.textContent = data.content;
                logViewerModal.classList.remove('hidden');
                break;
            case 'setup_status':
                setupManager.handleStatus(data.status);
                break;
            case 'pull_progress':
                setupManager.handleProgress(data);
                break;
            case 'log_deleted':
                // Refresh list
                socket.send(JSON.stringify({ type: 'open_logs' }));
                break;
        }
    };
}

function json_parse(str) { try { return JSON.parse(str); } catch (e) { return null; } }

// --- UI HELPERS ---
function updateStatusDisplay(status, modelName) {
    statusText.textContent = status;
    appBody.className = `status-${status}`;
    if (modelName) currentModelName = modelName;
    
    const modelBadge = document.getElementById('model-badge');
    if (modelBadge && modelName) {
        modelBadge.textContent = modelName.toUpperCase();
    }
    
    if (status === 'idle' || status === 'disconnected') {
        userInput.disabled = false;
        sendBtn.classList.remove('hidden');
        stopBtn.classList.add('hidden');
        if (status === 'idle') userInput.focus();
    } else {
        userInput.disabled = true;
        sendBtn.classList.add('hidden');
        stopBtn.classList.remove('hidden');
    }
}

function ensureTurnContainer() {
    if (!currentTurnContainer) {
        currentTurnContainer = document.createElement('div');
        currentTurnContainer.className = 'agent-turn';
        chatHistory.appendChild(currentTurnContainer);
        
        // Add Model Header for the turn
        const header = document.createElement('div');
        header.className = 'agent-badge';
        header.textContent = (currentModelName || 'Aether Core').toUpperCase();
        currentTurnContainer.appendChild(header);

        // Unified bubble for the turn
        currentAgentBubble = document.createElement('div');
        currentAgentBubble.className = 'agent-bubble';
        currentTurnContainer.appendChild(currentAgentBubble);

        // --- EVENT DELEGATION FOR REASONING ---
        currentTurnContainer.addEventListener('click', (e) => {
            const header = e.target.closest('.thought-header');
            if (header) {
                const container = header.closest('.thought-container');
                if (container) container.classList.toggle('open');
            }
        });
    }
}

function finalizeTurn() {
    currentTurnContainer = null;
    currentAgentBubble = null;
    currentThoughtContainer = null;
    currentToolGroup = null;
}

function scrollToBottom() {
    if (userScrolledUp) return;
    chatHistory.scrollTop = chatHistory.scrollHeight;
}

// --- MESSAGE RENDERING (STRICT CHRONOLOGY) ---

function appendSimpleMessage(role, content) {
    const div = document.createElement('div');
    div.className = `message ${role}`;
    const span = document.createElement('span');
    span.textContent = content;
    div.appendChild(span);
    chatHistory.appendChild(div);
    scrollToBottom();
}

/**
 * REASONING (The 'Thought' stream)
 */
function appendToThought(text, isFlush = false) {
    if (!currentTurnContainer) ensureTurnContainer();
    
    // If the last element in turn is NOT a thought container, OR we just reset it, create one.
    if (!currentThoughtContainer) {
        currentThoughtContainer = document.createElement('div');
        currentThoughtContainer.className = 'thought-container open';
        currentThoughtContainer.innerHTML = `
            <div class="thought-header"><span>REASONING</span></div>
            <div class="thought-content"></div>
        `;
        currentAgentBubble.appendChild(currentThoughtContainer);
    }
    
    const content = currentThoughtContainer.querySelector('.thought-content');
    if (isFlush) content.textContent = text;
    else content.textContent += text;
    scrollToBottom();
}

/**
 * TOOLS (The 'Capsules' / Kugeln)
 */
const TOOL_LABELS = {
    'web_search': 'Searching web',
    'fetch_url': 'Reading page',
    'open_app': 'Launching app',
    'get_all_windows': 'Locating windows',
    'run_command': 'Running command',
    'move_window': 'Moving window'
};

function createToolCapsule(name, args, call_id) {
    // BREAK reasoning stream: any future thoughts start a new block below this tool
    currentThoughtContainer = null;
    
    // Ensure we have a horizontal GROUP for capsules
    if (!currentToolGroup) {
        currentToolGroup = document.createElement('div');
        currentToolGroup.className = 'tool-group';
        currentAgentBubble.appendChild(currentToolGroup);
    }
    
    const capsule = document.createElement('div');
    capsule.className = 'tool-capsule working';
    const label = TOOL_LABELS[name] || name.toUpperCase();
    capsule.innerHTML = `<div class="status-dot"></div><span>${label}</span>`;
    
    const outputDiv = document.createElement('div');
    outputDiv.className = 'tool-capsule-output';
    outputDiv.innerHTML = `<pre>Args: ${JSON.stringify(args, null, 2)}</pre>`;
    
    capsule.onclick = () => {
        capsule.classList.toggle('open');
        outputDiv.classList.toggle('visible');
    };
    
    currentToolGroup.appendChild(capsule);
    currentAgentBubble.appendChild(outputDiv); 
    
    if (call_id) activeToolCapsules[call_id] = { capsule, outputDiv };
    scrollToBottom();
}

function updateToolCapsule(name, output, call_id) {
    const entry = call_id ? activeToolCapsules[call_id] : null;
    if (!entry) return;
    
    const { capsule, outputDiv } = entry;
    capsule.classList.remove('working');
    
    const isError = output && (output.status === 'error' || String(output.message || "").toUpperCase().includes("ERROR"));
    capsule.classList.add(isError ? 'error' : 'success');
    
    // Update Output
    const pre = outputDiv.querySelector('pre');
    const content = typeof output === 'string' ? output : JSON.stringify(output, null, 2);
    pre.textContent = `Output:\n${content}`;
    
    // Auto-collapse if very long? No, let user decide by clicking capsule.
    // But we can add a 'Close' button inside for convenience.
    if (!outputDiv.querySelector('.close-output')) {
        const closeBtn = document.createElement('div');
        closeBtn.className = 'close-output';
        closeBtn.textContent = 'CLOSE';
        closeBtn.onclick = (e) => {
            e.stopPropagation();
            capsule.classList.remove('open');
            outputDiv.classList.remove('visible');
        };
        outputDiv.appendChild(closeBtn);
    }
    
    scrollToBottom();
}

/**
 * RESPONSE (The final text)
 */
function appendAgentChunk(content) {
    if (!currentTurnContainer) ensureTurnContainer();
    
    // Breaking reasoning sequence
    currentThoughtContainer = null;
    
    // To maintain chronological order relative to tool capsules,
    // we only reuse the text block if it was the VERY LAST thing added to the bubble.
    let textEl = currentAgentBubble.lastElementChild;
    if (!textEl || !textEl.classList.contains('agent-text-content')) {
        textEl = document.createElement('div');
        textEl.className = 'agent-text-content';
        currentAgentBubble.appendChild(textEl);
    }
    
    const raw = (textEl.dataset.raw || "") + content;
    textEl.dataset.raw = raw;
    textEl.innerHTML = window.marked ? marked.parse(raw) : raw;
    scrollToBottom();
}

// --- CORE ACTIONS ---
function sendMessage() {
    const text = userInput.value.trim();
    if (!text || !socket || socket.readyState !== WebSocket.OPEN) return;
    
    finalizeTurn(); // Wrap up any previous turn
    appendSimpleMessage('user', text);
    socket.send(JSON.stringify({ type: 'user_message', content: text }));
    userInput.value = '';
}

function stopExecution() {
    updateStatusDisplay('idle');
    finalizeTurn();
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type: 'stop_request' }));
    }
}


// Event Listeners
sendBtn.addEventListener('click', sendMessage);
stopBtn.addEventListener('click', stopExecution);
logsBtn.addEventListener('click', () => {
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type: 'open_logs' }));
    }
});

const refreshLogsBtn = document.getElementById('refresh-logs-btn');
refreshLogsBtn.onclick = () => {
    if (socket && socket.readyState === WebSocket.OPEN) {
        refreshLogsBtn.classList.add('spinning');
        socket.send(JSON.stringify({ type: 'open_logs' }));
        setTimeout(() => refreshLogsBtn.classList.remove('spinning'), 500);
    }
};

closeLogsBtn.addEventListener('click', () => {
    logsModal.classList.add('hidden');
});
closeViewerBtn.addEventListener('click', () => {
    logViewerModal.classList.add('hidden');
});
pastebinInput.addEventListener('input', (e) => {
    // Silent save to backend
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ 
            type: 'update_settings', 
            settings: { pastebin_api_key: e.target.value } 
        }));
    }
});

function showLogsModal(logs, settings) {
    logsModal.classList.remove('hidden');
    pastebinInput.value = settings.pastebin_api_key || "";
    logsList.innerHTML = "";
    
    if (logs.length === 0) {
        logsList.innerHTML = "<div class='no-logs'>No session logs found.</div>";
        return;
    }
    
    logs.forEach(log => {
        const item = document.createElement('div');
        item.className = 'log-item';
        item.innerHTML = `
            <div class="log-info">
                <span class="log-name">${log.name}</span>
                <div class="log-meta">
                    <span class="log-size">${log.size}</span>
                    <span class="log-time">${log.time}</span>
                    ${log.is_active ? '<span class="status-label">(ACTIVE)</span>' : ''}
                </div>
            </div>
            <div class="log-actions">
                <button class="action-btn view-btn" data-path="${log.path}">VIEW</button>
                <button class="action-btn upload-btn" data-path="${log.path}">UPLOAD</button>
                <button class="action-btn delete-btn" data-path="${log.path}">DELETE</button>
            </div>
            <div class="log-result hidden" id="result-${log.path.replace(/[^a-zA-Z0-9]/g, '_')}"></div>
        `;
        
        item.querySelector('.view-btn').onclick = () => {
            socket.send(JSON.stringify({ type: 'get_log_content', path: log.path }));
        };

        if (log.is_active) {
            item.querySelector('.delete-btn').style.opacity = '0.3';
            item.querySelector('.delete-btn').style.pointerEvents = 'none';
        }
        
        item.querySelector('.delete-btn').onclick = (e) => {
            const btn = e.target;
            btn.disabled = true;
            btn.textContent = "DELETING...";
            socket.send(JSON.stringify({ type: 'delete_log_file', path: log.path }));
        };

        item.querySelector('.upload-btn').onclick = (e) => {
            const btn = e.target;
            const apiKey = pastebinInput.value.trim();
            if (!apiKey) {
                alert("Please enter a Pastebin API Key first!");
                return;
            }
            btn.disabled = true;
            btn.textContent = "UPLOADING...";
            socket.send(JSON.stringify({ type: 'upload_log_pastebin', path: log.path, api_key: apiKey }));
        };
        
        logsList.appendChild(item);
    });
}

function handlePastebinResult(result, path) {
    const safeId = "result-" + path.replace(/[^a-zA-Z0-9]/g, '_');
    const resultDiv = document.getElementById(safeId);
    if (!resultDiv) return;
    
    const uploadBtn = resultDiv.parentElement.querySelector('.upload-btn');
    uploadBtn.disabled = false;
    uploadBtn.textContent = "UPLOAD";

    resultDiv.classList.remove('hidden');
    if (result.status === 'success') {
        resultDiv.className = "log-result success";
        resultDiv.innerHTML = `
            <a href="${result.url}" target="_blank">${result.url}</a>
            <button class="mini-copy-btn" id="copy-link-${safeId}">COPY</button>
        `;
        
        const copyBtn = document.getElementById(`copy-link-${safeId}`);
        copyBtn.onclick = () => copyToClipboard(result.url, copyBtn);
        
        // Auto-copy for convenience
        copyToClipboard(result.url);
    } else {
        resultDiv.className = "log-result error";
        resultDiv.textContent = result.message;
    }
}
userInput.addEventListener('keypress', (e) => { if (e.key === 'Enter') sendMessage(); });

chatHistory.addEventListener('wheel', (e) => { if (e.deltaY < 0) userScrolledUp = true; }, { passive: true });
chatHistory.addEventListener('scroll', () => {
    const nearBottom = chatHistory.scrollHeight - chatHistory.scrollTop <= chatHistory.clientHeight + 60;
    if (nearBottom) userScrolledUp = false;
}, { passive: true });

// --- UTILS ---
async function copyToClipboard(text, btn = null) {
    try {
        await navigator.clipboard.writeText(text);
        if (btn) {
            const originalText = btn.textContent;
            btn.textContent = "COPIED!";
            btn.classList.add('success-state');
            setTimeout(() => {
                btn.textContent = originalText;
                btn.classList.remove('success-state');
            }, 1500);
        }
    } catch (err) {
        console.error("Failed to copy!", err);
    }
}

// --- GLOBAL SHORTCUTS ---
window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        // Hierarchical escape: close viewer first, then logs modal
        if (!logViewerModal.classList.contains('hidden')) {
            logViewerModal.classList.add('hidden');
        } else if (!logsModal.classList.contains('hidden')) {
            logsModal.classList.add('hidden');
        }
    }
});

copyLogBtn.onclick = () => {
    copyToClipboard(viewerContent.textContent, copyLogBtn);
};

class SetupManager {
    constructor() {
        this.status = null;
        this.isInstalling = false;
        this.hasInitialized = false;

        startSetupBtn.onclick = () => this.startInstallation();
        skipSetupBtn.onclick = () => this.finishSetup();
        fixDockerBtn.onclick = () => {
            socket.send(JSON.stringify({ type: 'fix_docker' }));
            fixDockerBtn.disabled = true;
            fixDockerBtn.textContent = "FIXING...";
        };
    }

    handleStatus(status) {
        this.status = status;
        console.log("[Setup] Status Update:", status);

        const models = status.models;
        const mainModel = models['qwen2.5:14b'];
        const transModel = models['translategemma:4b'];

        // Update UI Cards (Rows)
        this.updateCard('card-ollama', status.ollama);
        this.updateCard('card-docker', status.docker && status.searxng);
        this.updateCard('card-model-main', mainModel.installed);
        this.updateCard('card-model-translate', transModel.installed);

        // Show Fix button if docker/searxng is down
        if (status.docker && !status.searxng) {
            fixDockerBtn.classList.remove('hidden');
            fixDockerBtn.disabled = false;
            fixDockerBtn.textContent = "REPAIR";
        } else {
            fixDockerBtn.classList.add('hidden');
        }

        // --- Loading Disposal ---
        if (!this.hasInitialized) {
            this.hasInitialized = true;
            const loadingScreen = document.getElementById('loading-screen');
            if (loadingScreen) {
                loadingScreen.classList.add('fade-out');
                setTimeout(() => loadingScreen.remove(), 600);
            }
        }

        // Logic to switch screens
        const allReady = status.ollama && status.searxng && mainModel.installed && transModel.installed;
        
        if (allReady && !status.forced) {
            this.finishSetup();
        } else {
            this.showSetup();
            // Enable "Start Installation" if ollama is running but models are missing
            if (status.ollama && (!mainModel.installed || !transModel.installed)) {
                startSetupBtn.classList.remove('hidden');
            } else {
                startSetupBtn.classList.add('hidden');
            }
            
            if (status.forced) {
                skipSetupBtn.classList.remove('hidden');
            }
        }
    }

    updateCard(id, isReady) {
        const el = document.getElementById(id);
        const label = el.querySelector('.status-text-info');
        if (isReady) {
            el.className = "setup-row ready";
            label.textContent = "ALIVE";
        } else {
            el.className = "setup-row error";
            label.textContent = "MISSING";
        }
    }

    handleProgress(data) {
        progressWrapper.classList.remove('hidden');
        progressText.textContent = `[${data.model}] ${data.status}...`;
        progressPercent.textContent = `${Math.round(data.percent)}%`;
        progressBarFill.style.width = `${data.percent}%`;
    }

    async startInstallation() {
        if (this.isInstalling) return;
        this.isInstalling = true;
        startSetupBtn.disabled = true;
        startSetupBtn.textContent = "INSTALLING...";
        
        if (!this.status.models['qwen2.5:14b'].installed) {
            socket.send(JSON.stringify({ type: 'pull_model', model: 'qwen2.5:14b' }));
        } else if (!this.status.models['translategemma:4b'].installed) {
            socket.send(JSON.stringify({ type: 'pull_model', model: 'translategemma:4b' }));
        }
    }

    showSetup() {
        setupScreen.classList.remove('hidden');
        appContainer.classList.add('hidden');
        appBody.style.overflow = 'hidden';
    }

    finishSetup() {
        setupScreen.classList.add('hidden');
        appContainer.classList.remove('hidden');
        appBody.style.overflow = 'auto';
    }
}

const setupManager = new SetupManager();

connect();

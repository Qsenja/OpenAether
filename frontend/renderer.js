const chatHistory = document.getElementById('chat-history');
const userInput = document.getElementById('user-input');
const sendBtn = document.getElementById('send-btn');
const stopBtn = document.getElementById('stop-btn');
const statusText = document.getElementById('status-text');
const appBody = document.body;

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

    socket.onopen = () => updateStatusDisplay('idle');
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
        removeThinkingIndicator();
        if (status === 'idle') userInput.focus();
    } else {
        userInput.disabled = true;
        sendBtn.classList.add('hidden');
        stopBtn.classList.remove('hidden');
        ensureThinkingIndicator(status);
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

        // --- EVENT DELEGATION FOR REASONING ---
        // Instead of adding listeners to every header, we listen once per turn
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
        currentTurnContainer.appendChild(currentThoughtContainer);
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
    currentAgentBubble = null;
    
    // Ensure we have a horizontal GROUP for capsules
    if (!currentToolGroup) {
        currentToolGroup = document.createElement('div');
        currentToolGroup.className = 'tool-group';
        currentTurnContainer.appendChild(currentToolGroup);
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
    currentTurnContainer.appendChild(outputDiv); 
    
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
    
    // Breaking reasoning/tool sequence
    currentThoughtContainer = null;
    
    if (!currentAgentBubble) {
        currentAgentBubble = document.createElement('div');
        currentAgentBubble.className = 'agent-text';
        currentTurnContainer.appendChild(currentAgentBubble);
    }
    
    const raw = (currentAgentBubble.dataset.raw || "") + content;
    currentAgentBubble.dataset.raw = raw;
    currentAgentBubble.innerHTML = window.marked ? marked.parse(raw) : raw;
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

function ensureThinkingIndicator(status) {
    let indicator = document.getElementById('working-indicator');
    if (!indicator) {
        indicator = document.createElement('div');
        indicator.id = 'working-indicator';
        indicator.className = 'thinking-indicator';
        indicator.innerHTML = '<div class="thinking-dot"></div><span class="indicator-text"></span>';
        chatHistory.appendChild(indicator);
    }
    indicator.querySelector('.indicator-text').textContent = status.toUpperCase();
}

function removeThinkingIndicator() {
    const ind = document.getElementById('working-indicator');
    if (ind) ind.remove();
}

// Event Listeners
sendBtn.addEventListener('click', sendMessage);
stopBtn.addEventListener('click', stopExecution);
userInput.addEventListener('keypress', (e) => { if (e.key === 'Enter') sendMessage(); });

chatHistory.addEventListener('wheel', (e) => { if (e.deltaY < 0) userScrolledUp = true; }, { passive: true });
chatHistory.addEventListener('scroll', () => {
    const nearBottom = chatHistory.scrollHeight - chatHistory.scrollTop <= chatHistory.clientHeight + 60;
    if (nearBottom) userScrolledUp = false;
}, { passive: true });

connect();

import { useState, useEffect, useCallback, useRef } from 'react';
import { Command } from '@tauri-apps/plugin-shell';
import { invoke } from '@tauri-apps/api/core';

export type Block = 
  | { type: 'text'; content: string }
  | { type: 'thought'; content: string }
  | { type: 'tool_call'; call_id: string; name: string; args: string }
  | { type: 'tool_output'; call_id: string; name: string; output: any };

export interface Message {
  role: 'user' | 'assistant' | 'system' | 'function';
  content: string;
  thought?: string;
  toolCalls?: ToolCall[];
  toolOutputs?: ToolOutput[];
  sequence?: Block[];
  isSearching?: boolean;
}

export interface ToolCall {
  call_id: string;
  name: string;
  args: string;
}

export interface ToolOutput {
  call_id: string;
  name: string;
  output: any;
}

export function useBackend() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [status, setStatus] = useState<'idle' | 'thinking' | 'executing' | 'error'>('idle');
  const [isConnected, setIsConnected] = useState(false);
  const [isInitializing, setIsInitializing] = useState(true);
  const ws = useRef<WebSocket | null>(null);
  const backendProcess = useRef<any>(null);
  const isSpawning = useRef(false);
  const isMounted = useRef(true);

  const logToTerminal = async (msg: string) => {
    try {
      await invoke('log_message', { message: `[FRONTEND] ${msg}` });
    } catch (e) {
      console.log(`[FRONTEND] ${msg}`);
    }
  };

  const connect = useCallback(() => {
    const socket = new WebSocket('ws://localhost:8765');
    ws.current = socket;

    socket.onopen = () => {
      logToTerminal('WebSocket connected');
      setIsConnected(true);
      setIsInitializing(false);
    };

    socket.onclose = () => {
      logToTerminal('WebSocket closed');
      setIsConnected(false);
      
      // Auto-reconnect logic: Wait 2 seconds and try again 
      // if it wasn't a deliberate stop and we aren't currently initializing
      setTimeout(() => {
        if (isMounted.current) {
          logToTerminal('Attempting to reconnect...');
          connect();
        }
      }, 2000);
    };

    socket.onmessage = (event) => {
      const data = JSON.parse(event.data);
      handleBackendMessage(data);
    };

    socket.onerror = (err) => {
      logToTerminal(`WebSocket error: ${err}`);
      setIsConnected(false);
    };
  }, []);

  const spawnBackend = async () => {
    if (isSpawning.current) return;
    isSpawning.current = true;

    try {
      logToTerminal('Spawning backend from venv...');
      // Use the alias 'venv-python' defined in capabilities/default.json
      const command = Command.create('venv-python', ['../../backend/main.py']);
      
      command.stdout.on('data', line => {
        invoke('log_message', { message: `[BACKEND] ${line}` });
      });
      
      command.stderr.on('data', line => {
        invoke('log_message', { message: `[BACKEND ERROR] ${line}` });
      });

      const child = await command.spawn();
      backendProcess.current = child;
      
      // Wait a bit for the WS server to start
      setTimeout(connect, 3000);
    } catch (err) {
      logToTerminal(`Spawn error: ${err}`);
      setIsInitializing(false);
      setStatus('error');
    } finally {
      isSpawning.current = false;
    }
  };

  const retry = useCallback(() => {
    setIsInitializing(true);
    if (ws.current) ws.current.close();
    spawnBackend();
  }, []);

  useEffect(() => {
    let mounted = true;

    const init = async () => {
      if (!isSpawning.current && !backendProcess.current) {
        await spawnBackend();
      } else if (backendProcess.current && !isConnected && !isInitializing) {
        // If process exists but no connection, try connecting
        connect();
      }
    };

    init();
    isMounted.current = true;

    return () => {
      isMounted.current = false;
      // In production, we might want to kill the process on unmount,
      // but in dev, we let it persist to avoid double-spawn cycles.
      // We only close the websocket if the app is truly closing.
    };
  }, []);

  const handleBackendMessage = (data: any) => {
    logToTerminal(`Received event: ${data.type}`);
    switch (data.type) {
      case 'status_update':
        setStatus(data.status);
        break;

      case 'agent_message':
        setMessages((prev) => {
          if (prev.length === 0) {
            return [{ 
              role: 'assistant', 
              content: data.content, 
              sequence: [{ type: 'text', content: data.content }] 
            }];
          }
          const last = prev[prev.length - 1];
          if (last.role === 'assistant') {
            const seq = [...(last.sequence || [])];
            const lastIdx = seq.length - 1;
            const lastBlock = seq[lastIdx];
            
            if (lastBlock && lastBlock.type === 'text') {
              seq[lastIdx] = { ...lastBlock, content: lastBlock.content + data.content };
            } else {
              seq.push({ type: 'text', content: data.content });
            }
            
            return [
              ...prev.slice(0, -1),
              { ...last, content: last.content + data.content, sequence: seq },
            ];
          } else {
            return [...prev, { 
              role: 'assistant', 
              content: data.content, 
              sequence: [{ type: 'text', content: data.content }] 
            }];
          }
        });
        break;

      case 'agent_thought_chunk':
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last && last.role === 'assistant') {
            const seq = [...(last.sequence || [])];
            const lastIdx = seq.length - 1;
            const lastBlock = seq[lastIdx];
            
            if (lastBlock && lastBlock.type === 'thought') {
              // Create a NEW block object instead of mutating
              seq[lastIdx] = { ...lastBlock, content: lastBlock.content + data.content };
            } else {
              seq.push({ type: 'thought', content: data.content });
            }
            
            return [
              ...prev.slice(0, -1),
              { ...last, thought: (last.thought || '') + data.content, sequence: seq },
            ];
          } else {
            return [...prev, { 
              role: 'assistant', 
              content: '', 
              thought: data.content,
              sequence: [{ type: 'thought', content: data.content }]
            }];
          }
        });
        break;

      case 'tool_call':
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          const newToolCall = { call_id: data.call_id, name: data.name, args: data.args };
          const toolBlock: Block = { type: 'tool_call', ...newToolCall };
          
          if (last && last.role === 'assistant') {
            const seq = [...(last.sequence || []), toolBlock];
            return [
              ...prev.slice(0, -1),
              { 
                ...last, 
                toolCalls: [...(last.toolCalls || []), newToolCall],
                sequence: seq 
              },
            ];
          } else {
            return [...prev, { 
              role: 'assistant', 
              content: '', 
              toolCalls: [newToolCall],
              sequence: [toolBlock]
            }];
          }
        });
        break;

      case 'tool_output':
        setMessages((prev) => {
          return prev.map((msg) => {
            if (msg.role === 'assistant' && (msg.toolCalls?.some(tc => tc.call_id === data.call_id) || msg.sequence?.some(b => b.type === 'tool_call' && b.call_id === data.call_id))) {
              const outputBlock: Block = { 
                type: 'tool_output', 
                call_id: data.call_id, 
                name: data.name, 
                output: data.output 
              };
              const seq = [...(msg.sequence || []), outputBlock];
              return {
                ...msg,
                toolOutputs: [...(msg.toolOutputs || []), { call_id: data.call_id, name: data.name, output: data.output }],
                sequence: seq
              };
            }
            return msg;
          });
        });
        break;

      case 'agent_message_done':
        setStatus('idle');
        break;

      case 'error':
        setStatus('error');
        logToTerminal(`Backend error: ${data.content}`);
        break;

      default:
        break;
    }
  };

  const sendMessage = useCallback((content: string) => {
    if (ws.current && ws.current.readyState === WebSocket.OPEN) {
      setMessages((prev) => [...prev, { role: 'user', content }]);
      ws.current.send(JSON.stringify({ type: 'user_message', content }));
    }
  }, []);

  const stopGeneration = useCallback(() => {
    if (ws.current && ws.current.readyState === WebSocket.OPEN) {
      ws.current.send(JSON.stringify({ type: 'stop_request' }));
    }
  }, []);

  return { messages, status, isConnected, isInitializing, sendMessage, stopGeneration, retry };
}

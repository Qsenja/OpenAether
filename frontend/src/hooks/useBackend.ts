import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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

export interface UserSettings {
  pastebin_api_key: String;
  ollama_model: String;
  searxng_url: String;
  log_level: number;
  temperature: number;
  top_p: number;
}

export function useBackend() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [status, setStatus] = useState<'idle' | 'thinking' | 'executing' | 'error'>('idle');
  const [isConnected] = useState(true);
  const [isInitializing, setIsInitializing] = useState(true);
  const [settings, setSettings] = useState<UserSettings | null>(null);
  const isMounted = useRef(true);

  const logToTerminal = async (msg: string) => {
    try {
      await invoke('log_message', { message: msg });
    } catch (e) {
      console.log(msg);
    }
  };

  useEffect(() => {
    isMounted.current = true;
    
    // Listen for backend events
    const unlisten = listen<any>('backend-event', (event) => {
      handleBackendMessage(event.payload);
    });

    // Initial status check
    const checkStatus = async () => {
      try {
        await invoke<any>('get_setup_status');
        const initialSettings = await invoke<UserSettings>('get_settings');
        setSettings(initialSettings);
        setIsInitializing(false);
      } catch (e) {
        logToTerminal(`Initial status check failed: ${e}`);
        setIsInitializing(false);
      }
    };

    checkStatus();

    return () => {
      isMounted.current = false;
      unlisten.then(fn => fn());
    };
  }, []);

  const handleBackendMessage = (data: any) => {
    // logToTerminal(`Received event: ${data.type}`);
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

  const sendMessage = useCallback(async (content: string) => {
    setMessages((prev) => [...prev, { role: 'user', content }]);
    setStatus('thinking');
    
    try {
      // Map frontend messages to Ollama format for the Rust call
      const history = messages.map(m => ({
        role: m.role === 'function' ? 'tool' : m.role,
        content: m.content
      }));

      await invoke('send_message', { content, history });
    } catch (e: any) {
      logToTerminal(`Send Error: ${e}`);
      setStatus('error');
    }
  }, [messages]);

  const stopGeneration = useCallback(async () => {
    try {
      await invoke('stop_generation');
    } catch (e: any) {
      logToTerminal(`Stop Generation Error: ${e}`);
    }
  }, []);

  const retry = useCallback(() => {
    setIsInitializing(true);
    window.location.reload();
  }, []);

  const updateSettings = useCallback(async (newSettings: UserSettings) => {
    try {
      await invoke('update_settings', { settings: newSettings });
      setSettings(newSettings);
      logToTerminal('Settings updated successfully');
    } catch (e: any) {
      logToTerminal(`Update Settings Error: ${e}`);
    }
  }, []);

  return { 
    messages, 
    status, 
    isConnected, 
    isInitializing, 
    settings,
    sendMessage, 
    stopGeneration, 
    retry,
    updateSettings
  };
}

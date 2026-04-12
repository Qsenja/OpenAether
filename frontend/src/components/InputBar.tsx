import React, { useState, useRef, useEffect } from 'react';

interface InputBarProps {
  onSendMessage: (content: string) => void;
  onStop: () => void;
  status: 'idle' | 'thinking' | 'executing' | 'error';
  isConnected: boolean;
}

export const InputBar: React.FC<InputBarProps> = ({ onSendMessage, onStop, status, isConnected }) => {
  const [text, setText] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSend = () => {
    if (text.trim() && (status === 'idle' || status === 'error') && isConnected) {
      onSendMessage(text.trim());
      setText('');
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
    }
  }, [text]);

  const isBusy = status === 'thinking' || status === 'executing';

  return (
    <div style={{
      padding: '8px 20px',
      maxWidth: '750px',
      width: '100%',
      margin: '0 auto',
      display: 'flex',
      flexDirection: 'column',
      gap: '8px'
    }}>
      <div className="glass" style={{
        borderRadius: '12px',
        padding: '8px 14px',
        display: 'flex',
        alignItems: 'flex-end',
        gap: '10px',
        boxShadow: '0 4px 15px rgba(0, 0, 0, 0.3)',
        border: '1px solid rgba(47, 160, 132, 0.2)',
        opacity: isConnected ? 1 : 0.6,
        pointerEvents: isConnected ? 'auto' : 'none'
      }}>
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isConnected ? "Message Aether Core..." : "Offline - Waiting for backend..."}
          disabled={!isConnected}
          style={{
            flex: 1,
            background: 'transparent',
            border: 'none',
            outline: 'none',
            color: 'var(--text-main)',
            fontFamily: 'var(--font-main)',
            fontSize: '0.95rem',
            resize: 'none',
            maxHeight: '150px',
            padding: '6px 0',
            lineHeight: '1.4'
          }}
        />
        
        <button
          onClick={isBusy ? onStop : handleSend}
          disabled={!isConnected && !isBusy}
          style={{
            background: isBusy ? '#ff4d4d' : 'var(--primary)',
            color: 'white',
            border: 'none',
            borderRadius: '10px',
            width: '32px',
            height: '32px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            cursor: 'pointer',
            transition: 'all 0.2s ease',
            flexShrink: 0,
            marginBottom: '3px',
            opacity: isConnected || isBusy ? 1 : 0.5
          }}
          title={isBusy ? "Stop Generation" : "Send Message"}
        >
          {isBusy ? (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          ) : (
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M22 2L11 13" />
              <path d="M22 2L15 22L11 13L2 9L22 2Z" />
            </svg>
          )}
        </button>
      </div>
      <div style={{
        fontSize: '0.7rem',
        color: 'var(--text-dim)',
        textAlign: 'center',
        fontFamily: 'var(--font-main)',
        opacity: 0.5
      }}>
        OpenAether can make mistakes. Check important info.
      </div>
    </div>
  );
};

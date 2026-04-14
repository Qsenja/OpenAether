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
      textareaRef.current.style.height = '24px'; // Base height
      if (text) {
        textareaRef.current.style.height = 'auto';
        textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
      }
    }
  }, [text]);

  const isBusy = status === 'thinking' || status === 'executing';

  return (
    <div style={{
      padding: '0 24px 12px',
      margin: '0 auto',
      width: '100%',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      gap: '4px',
      animation: 'fadeIn 0.6s ease-out'
    }}>
      <div className="glass slim-input-fix" style={{
        borderRadius: 'var(--radius-pill)',
        width: '100%',
        transition: 'all 0.3s ease',
        backdropFilter: 'blur(30px)',
        gap: '12px'
      }}>
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isConnected ? "Message Fabel..." : "Connecting..."}
          disabled={!isConnected}
          style={{
            flex: 1,
            background: 'transparent',
            border: 'none',
            outline: 'none',
            color: 'var(--text-main)',
            fontFamily: 'var(--font-main)',
            fontSize: '1rem',
            resize: 'none',
            maxHeight: '150px',
            padding: '0',
            lineHeight: '24px',
            height: '24px',
            fontWeight: 400
          }}
        />
        
        <button
          onClick={isBusy ? onStop : handleSend}
          disabled={!isConnected && !isBusy}
          style={{
            background: isBusy ? 'var(--danger)' : 'var(--primary)',
            color: 'white',
            border: 'none',
            borderRadius: '50%',
            width: '32px',
            height: '32px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            cursor: 'pointer',
            transition: 'all 0.2s ease',
            flexShrink: 0,
            opacity: isConnected || isBusy ? 1 : 0.5,
            boxShadow: '0 2px 8px rgba(0, 0, 0, 0.2)'
          }}
        >
          {isBusy ? (
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          ) : (
            <svg width="18" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" style={{ transform: 'translateX(1px)' }}>
              <path d="M22 2L11 13" />
              <path d="M22 2L15 22L11 13L2 9L22 2Z" />
            </svg>
          )}
        </button>
      </div>
      <div style={{
        fontSize: '0.65rem',
        color: 'var(--text-dim)',
        opacity: 0.2
      }}>
        OpenAether
      </div>
    </div>
  );
};

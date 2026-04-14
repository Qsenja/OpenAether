import React, { useRef, useEffect } from 'react';
import { Message } from '../hooks/useBackend';
import { MessageItem } from './MessageItem';

interface MessageListProps {
  messages: Message[];
  status: 'idle' | 'thinking' | 'executing' | 'error';
}

export const MessageList: React.FC<MessageListProps> = ({ messages, status }) => {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  return (
    <div 
      ref={scrollRef}
      style={{
        flex: 1,
        overflowY: 'auto',
        padding: '0 20px',
        paddingTop: '80px',
        display: 'flex',
        flexDirection: 'column',
        scrollBehavior: 'smooth'
      }}
    >
      <div style={{ maxWidth: '800px', width: '100%', margin: '0 auto' }}>
        {messages.length === 0 && (
          <div style={{
            height: '100%',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            marginTop: '10vh',
            opacity: 0.5,
            textAlign: 'center'
          }}>
            <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
            <p style={{ marginTop: '20px', fontSize: '1.2rem', fontFamily: 'var(--font-logo)' }}>
              How can Fabel help you today?
            </p>
          </div>
        )}
        {messages.map((msg, index) => (
          <MessageItem key={index} message={msg} status={status} isLast={index === messages.length - 1} />
        ))}
        {status === 'thinking' && (messages.length === 0 || messages[messages.length - 1].role !== 'assistant') && (
          <MessageItem 
            message={{ role: 'assistant', content: '', thought: '' }} 
            status={status} 
            isLast={true} 
          />
        )}
      </div>
    </div>
  );
};

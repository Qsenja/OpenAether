import React from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Message, Block } from '../hooks/useBackend';
import { ToolTile } from './ToolTile';

interface MessageItemProps {
  message: Message;
  status: 'idle' | 'thinking' | 'executing' | 'error';
  isLast: boolean;
}

export const MessageItem: React.FC<MessageItemProps> = ({ message, status, isLast }) => {
  const isAssistant = message.role === 'assistant';

  if (!isAssistant) {
    return (
      <div style={{
        display: 'flex',
        justifyContent: 'flex-end',
        margin: '24px 0',
        animation: 'fadeIn 0.4s ease-out'
      }}>
        <div className="user-bubble-fix" style={{
          maxWidth: '85%',
          color: 'var(--text-main)',
          lineHeight: '1.6',
          fontSize: '0.9rem',
          boxShadow: 'var(--shadow-sm)'
        }}>
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.content}
          </ReactMarkdown>
        </div>
      </div>
    );
  }

  const renderBlock = (block: Block, index: number, isLastBlock: boolean) => {
    switch (block.type) {
      case 'text':
        return (
          <div key={index} className="markdown-content" style={{ 
            marginBottom: '12px',
            color: 'var(--text-main)',
            opacity: 0.95,
            display: 'inline-block',
            width: '100%'
          }}>
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {block.content}
            </ReactMarkdown>
            {isLastBlock && isLast && (status === 'thinking' || status === 'executing') && (
              <span className="blinking-cursor" />
            )}
          </div>
        );
      
      case 'thought':
        return (
          <div key={index} style={{
            fontFamily: 'var(--font-main)',
            color: 'var(--text-dim)',
            fontSize: '0.85rem',
            padding: '2px 14px',
            borderLeft: '2px solid var(--primary)',
            marginBottom: '12px',
            opacity: 0.7,
            animation: status === 'thinking' ? 'pulse-soft 2s infinite' : 'none'
          }}>
            {block.content}
          </div>
        );

      case 'tool_call':
        const outputBlock = message.sequence?.find(b => b.type === 'tool_output' && b.call_id === block.call_id) as any;
        return (
          <ToolTile 
            key={index} 
            toolCall={block} 
            toolOutput={outputBlock} 
          />
        );

      case 'tool_output':
        return null;

      default:
        return null;
    }
  };

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      margin: '18px 0',
      maxWidth: '92%',
      animation: 'fadeIn 0.5s ease-out'
    }}>
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: '12px',
        marginBottom: '14px'
      }}>
        <div style={{
          width: '32px',
          height: '32px',
          borderRadius: '12px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: 'var(--primary)',
          boxShadow: '0 0 15px var(--primary-glow)'
        }}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.5">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
        </div>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: '8px' }}>
          <span style={{ 
            color: 'var(--primary)', 
            fontWeight: 700, 
            fontSize: '0.9rem',
            letterSpacing: '0.02rem',
            textShadow: '0 0 10px var(--primary-glow)'
          }}>
            Fabel
          </span>
        </div>
      </div>

      <div className="ai-ambient-text" style={{
        lineHeight: '1.6',
        fontSize: '0.9rem'
      }}>
        {message.sequence ? (
          message.sequence.map((block, i) => renderBlock(block, i, i === message.sequence!.length - 1))
        ) : (
          <div className="markdown-content">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {message.content}
            </ReactMarkdown>
            {isLast && (status === 'thinking' || status === 'executing') && (
              <span className="blinking-cursor" />
            )}
          </div>
        )}
      </div>


    </div>
  );
};

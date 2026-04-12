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
        margin: '16px 0',
      }}>
        <div style={{
          maxWidth: '85%',
          padding: '12px 18px',
          borderRadius: '20px 20px 4px 20px',
          color: 'var(--text-main)',
          background: 'rgba(249, 115, 22, 0.1)', /* Orange tint */
          border: '1px solid var(--glass-border)',
          lineHeight: '1.6',
          fontSize: '0.95rem',
          boxShadow: 'var(--shadow-sm)'
        }}>
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.content}
          </ReactMarkdown>
        </div>
      </div>
    );
  }

  const renderBlock = (block: Block, index: number) => {
    switch (block.type) {
      case 'text':
        return (
          <div key={index} className="markdown-content" style={{ 
            marginBottom: '12px',
            color: 'var(--text-main)',
            opacity: 0.95
          }}>
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {block.content}
            </ReactMarkdown>
            {isLast && index === (message.sequence?.length || 0) - 1 && (status === 'thinking' || status === 'executing') && (
              <span className="cursor">_</span>
            )}
          </div>
        );
      
      case 'thought':
        return (
          <div key={index} style={{
            fontFamily: 'var(--font-thinking)',
            color: 'var(--text-dim)',
            fontSize: '0.85rem',
            padding: '12px 16px',
            borderLeft: '2px solid var(--primary)',
            background: 'rgba(255, 255, 255, 0.02)',
            marginBottom: '16px',
            fontStyle: 'italic',
            borderRadius: '0 8px 8px 0',
            opacity: 0.8
          }}>
            {block.content}
            {isLast && index === (message.sequence?.length || 0) - 1 && status === 'thinking' && (
              <span className="cursor">_</span>
            )}
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
      margin: '24px 0',
      maxWidth: '90%'
    }}>
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: '10px',
        marginBottom: '12px'
      }}>
        <div className="glass" style={{
          width: '32px',
          height: '32px',
          borderRadius: '10px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          border: '1px solid var(--primary)'
        }}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--primary)" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <span style={{ 
            color: 'var(--primary)', 
            fontWeight: 800, 
            fontSize: '0.75rem',
            textTransform: 'uppercase',
            letterSpacing: '0.1rem'
          }}>
            Aether Core
          </span>
          <span style={{ fontSize: '0.65rem', color: 'var(--text-dim)', opacity: 0.6 }}>
            Intelligence Engine v2.5
          </span>
        </div>
      </div>

      <div style={{
        paddingLeft: '42px',
        lineHeight: '1.7',
        fontSize: '1rem'
      }}>
        {message.sequence ? (
          message.sequence.map((block, i) => renderBlock(block, i))
        ) : (
          <div className="markdown-content">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {message.content}
            </ReactMarkdown>
            {isLast && (status === 'thinking' || status === 'executing') && (
              <span className="cursor">_</span>
            )}
          </div>
        )}
      </div>

      <style>{`
        @keyframes blink {
          0%, 100% { opacity: 0; }
          50% { opacity: 1; }
        }
        .cursor {
          display: inline-block;
          color: var(--primary);
          margin-left: 4px;
          font-weight: bold;
          animation: blink 1s infinite;
        }
        .markdown-content p {
          margin-bottom: 12px;
        }
        .markdown-content p:last-child {
          margin-bottom: 0;
        }
        .markdown-content a {
          color: var(--primary);
          text-decoration: none;
          font-weight: 600;
          border-bottom: 1px solid transparent;
          transition: border-color 0.2s;
        }
        .markdown-content a:hover {
          border-color: var(--primary);
        }
        .markdown-content ul, .markdown-content ol {
          margin: 12px 0;
          padding-left: 20px;
        }
        .markdown-content li {
          margin-bottom: 6px;
        }
        .markdown-content code {
          background: rgba(255,255,255,0.08);
          padding: 2px 6px;
          border-radius: 4px;
          font-family: var(--font-mono);
          font-size: 0.85em;
        }
      `}</style>
    </div>
  );
};

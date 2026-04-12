import React, { useState } from 'react';
import { ToolCall, ToolOutput } from '../hooks/useBackend';

interface ToolTileProps {
  toolCall: ToolCall;
  toolOutput?: ToolOutput;
}

export const ToolTile: React.FC<ToolTileProps> = ({ toolCall, toolOutput }) => {
  const [isExpanded, setIsExpanded] = useState(false);
  const isDone = !!toolOutput;

  return (
    <div 
      className="sleek-card" 
      style={{
        margin: '12px 0',
        overflow: 'hidden',
        background: isDone ? 'rgba(74, 222, 128, 0.03)' : 'rgba(47, 160, 132, 0.03)',
        borderLeft: `3px solid ${isDone ? 'var(--success)' : 'var(--primary)'}`
      }}
    >
      <div 
        onClick={() => setIsExpanded(!isExpanded)}
        style={{
          padding: '12px 16px',
          display: 'flex',
          alignItems: 'center',
          gap: '12px',
          cursor: 'pointer',
          userSelect: 'none'
        }}
      >
        <div style={{
          width: '20px',
          height: '20px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: isDone ? 'var(--success)' : 'var(--primary)',
          opacity: 0.9
        }}>
          {isDone ? (
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
              <polyline points="22 4 12 14.01 9 11.01" />
            </svg>
          ) : (
            <svg className="spin" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
          )}
        </div>

        <div style={{ flex: 1 }}>
          <div style={{ 
            fontSize: '0.85rem', 
            fontWeight: 600, 
            color: 'var(--text-main)',
            opacity: 0.9,
            letterSpacing: '0.02rem',
            fontFamily: 'var(--font-mono)' 
          }}>
            {toolCall.name} <span style={{ opacity: 0.4, fontWeight: 400 }}>({isDone ? 'completed' : 'running...'})</span>
          </div>
        </div>

        <svg 
          style={{ 
            transform: isExpanded ? 'rotate(180deg)' : 'none', 
            transition: 'transform 0.3s cubic-bezier(0.4, 0, 0.2, 1)',
            opacity: 0.5
          }}
          width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </div>

      <div style={{
        maxHeight: isExpanded ? '500px' : '0',
        transition: 'max-height 0.4s cubic-bezier(0.4, 0, 0.2, 1)',
        overflow: 'hidden',
        background: 'rgba(0,0,0,0.2)'
      }}>
        <div style={{
          padding: '12px 16px',
          fontSize: '0.75rem',
          fontFamily: 'var(--font-mono)',
          color: 'var(--text-dim)',
          borderTop: '1px solid var(--glass-border)'
        }}>
          <div style={{ marginBottom: '10px' }}>
            <div style={{ color: 'var(--primary)', marginBottom: '4px', textTransform: 'uppercase', fontSize: '0.65rem', fontWeight: 700 }}>Arguments</div>
            <pre style={{ whiteSpace: 'pre-wrap', color: 'var(--text-main)', opacity: 0.8 }}>{toolCall.args}</pre>
          </div>
          {toolOutput && (
            <div>
              <div style={{ color: 'var(--success)', marginBottom: '4px', textTransform: 'uppercase', fontSize: '0.65rem', fontWeight: 700 }}>Output</div>
              <pre style={{ 
                whiteSpace: 'pre-wrap', 
                color: 'var(--secondary)', 
                opacity: 0.9,
                padding: '8px',
                background: 'rgba(0,0,0,0.3)',
                borderRadius: '6px'
              }}>
                {typeof toolOutput.output === 'string' ? toolOutput.output : JSON.stringify(toolOutput.output, null, 2)}
              </pre>
            </div>
          )}
        </div>
      </div>

      <style>{`
        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
        .spin {
          animation: spin 1s linear infinite;
        }
      `}</style>
    </div>
  );
};

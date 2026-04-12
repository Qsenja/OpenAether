import React, { useState, useEffect } from 'react';

interface LoadingScreenProps {
  onRetry?: () => void;
}

export const LoadingScreen: React.FC<LoadingScreenProps> = ({ onRetry }) => {
  const [showRetry, setShowRetry] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setShowRetry(true), 12000); // Show retry after 12 seconds
    return () => clearTimeout(timer);
  }, []);

  return (
    <div style={{
      height: '100vh',
      width: '100vw',
      background: 'var(--bg-dark)', // Solid #091413
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      position: 'fixed',
      top: 0,
      left: 0,
      zIndex: 10000,
      gap: '40px'
    }}>
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: '20px'
      }}>
        <h1 style={{
          fontFamily: 'var(--font-logo)',
          fontSize: '3rem',
          letterSpacing: '0.5rem',
          color: 'var(--accent)',
          textShadow: '0 0 20px rgba(111, 207, 151, 0.4)',
          animation: 'pulse-glow 3s infinite ease-in-out',
          margin: 0
        }}>
          OPENAETHER
        </h1>
        <div style={{
          fontFamily: 'var(--font-main)',
          color: 'var(--text-dim)',
          letterSpacing: '0.1rem',
          fontSize: '0.85rem',
          opacity: 0.8
        }}>
          {showRetry ? 'CONNECTION TIMEOUT: CHECK TERMINAL' : 'INITIALIZING INTELLIGENCE LAYER'}
        </div>
      </div>

      {!showRetry ? (
        <div style={{
          width: '180px',
          height: '2px',
          background: 'rgba(47, 160, 132, 0.15)',
          borderRadius: '1px',
          position: 'relative',
          overflow: 'hidden'
        }}>
          <div style={{
            position: 'absolute',
            top: 0,
            left: 0,
            height: '100%',
            width: '40%',
            background: 'var(--accent)',
            boxShadow: '0 0 10px var(--accent)',
            animation: 'scan 2s infinite linear'
          }} />
        </div>
      ) : (
        <button 
          onClick={onRetry}
          style={{
            background: 'transparent',
            border: '1px solid var(--accent)',
            color: 'var(--accent)',
            padding: '10px 24px',
            borderRadius: '4px',
            cursor: 'pointer',
            fontFamily: 'var(--font-main)',
            fontSize: '0.9rem',
            letterSpacing: '0.1rem',
            transition: 'all 0.2s'
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'rgba(111, 207, 151, 0.1)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'transparent';
          }}
        >
          RETRY INITIALIZATION
        </button>
      )}

      <style>{`
        @keyframes pulse-glow {
          0%, 100% { 
            transform: scale(1);
            opacity: 0.8;
          }
          50% { 
            transform: scale(1.01);
            opacity: 1;
          }
        }
        @keyframes scan {
          0% { transform: translateX(-100%); }
          100% { transform: translateX(250%); }
        }
      `}</style>
    </div>
  );
};

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
      background: 'var(--bg-dark)',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      position: 'fixed',
      top: 0,
      left: 0,
      zIndex: 10000,
      gap: '48px'
    }}>
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: '24px'
      }}>
        <h1 style={{
          fontFamily: 'var(--font-logo)',
          fontSize: '2.8rem',
          fontWeight: 800,
          letterSpacing: '0.5rem',
          color: 'var(--primary)',
          animation: 'pulse-glow 4s infinite ease-in-out',
          margin: 0,
          textShadow: '0 0 30px var(--primary-glow)'
        }}>
          OPENAETHER
        </h1>
        <div style={{
          fontFamily: 'var(--font-main)',
          color: 'var(--text-dim)',
          letterSpacing: '0.1rem',
          fontSize: '0.95rem',
          opacity: 0.6,
          textTransform: 'uppercase'
        }}>
          {showRetry ? 'CONNECTION TIMEOUT' : 'Establishing Synchrony...'}
        </div>
      </div>

      {!showRetry ? (
        <div style={{
          width: '240px',
          height: '1px',
          background: 'rgba(255, 255, 255, 0.05)',
          borderRadius: '1px',
          position: 'relative',
          overflow: 'hidden'
        }}>
          <div style={{
            position: 'absolute',
            top: 0,
            left: 0,
            height: '100%',
            width: '60px',
            background: 'linear-gradient(90deg, transparent, var(--primary), transparent)',
            boxShadow: '0 0 15px var(--primary-glow)',
            animation: 'scan 2.5s infinite ease-in-out'
          }} />
        </div>
      ) : (
        <button 
          onClick={onRetry}
          style={{
            background: 'var(--primary)',
            border: 'none',
            color: 'white',
            padding: '14px 36px',
            borderRadius: 'var(--radius-md)',
            cursor: 'pointer',
            fontFamily: 'var(--font-main)',
            fontSize: '1rem',
            fontWeight: 700,
            letterSpacing: '0.05rem',
            transition: 'all 0.3s cubic-bezier(0.4, 0, 0.2, 1)',
            boxShadow: '0 10px 20px rgba(0, 0, 0, 0.3)'
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = 'translateY(-2px)';
            e.currentTarget.style.boxShadow = '0 15px 30px rgba(0, 0, 0, 0.4)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'translateY(0)';
            e.currentTarget.style.boxShadow = '0 10px 20px rgba(0, 0, 0, 0.3)';
          }}
        >
          RETRY CONNECTION
        </button>
      )}

      <style>{`
        @keyframes pulse-glow {
          0%, 100% { 
            transform: scale(1);
            filter: brightness(1);
          }
          50% { 
            transform: scale(1.01);
            filter: brightness(1.2);
          }
        }
        @keyframes scan {
          0% { left: -30%; }
          100% { left: 110%; }
        }
      `}</style>
    </div>
  );
};

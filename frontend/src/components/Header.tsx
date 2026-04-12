import React from 'react';

export const Header: React.FC = () => {
  return (
    <header className="frameless-drag" style={{ 
      display: 'flex', 
      justifyContent: 'center', 
      alignItems: 'center',
      padding: '20px',
      background: 'transparent',
      height: '80px',
      userSelect: 'none'
    }}>
      <h1 style={{ 
        fontFamily: 'var(--font-logo)',
        fontSize: '1.5rem',
        letterSpacing: '0.2rem',
        color: 'var(--accent)',
        textShadow: '0 0 10px rgba(111, 207, 151, 0.3)',
        margin: 0
      }}>
        OPENAETHER
      </h1>
    </header>
  );
};

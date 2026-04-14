import React, { useState, useEffect } from 'react';
import { UserSettings } from '../hooks/useBackend';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  settings: UserSettings | null;
  onSave: (settings: UserSettings) => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({ isOpen, onClose, settings, onSave }) => {
  const [localSettings, setLocalSettings] = useState<UserSettings | null>(null);

  useEffect(() => {
    if (settings) {
      setLocalSettings({ ...settings });
    }
  }, [settings, isOpen]);

  if (!isOpen || !localSettings) return null;

  const handleSave = () => {
    onSave(localSettings);
    onClose();
  };

  const updateField = (field: keyof UserSettings, value: any) => {
    setLocalSettings(prev => prev ? { ...prev, [field]: value } : null);
  };

  return (
    <div style={{
      position: 'fixed',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      backgroundColor: 'rgba(0, 0, 0, 0.7)',
      backdropFilter: 'blur(8px)',
      display: 'flex',
      justifyContent: 'center',
      alignItems: 'center',
      zIndex: 1000,
      animation: 'fadeIn 0.2s ease-out'
    }} onClick={onClose}>
      <div className="glass" style={{
        width: '90%',
        maxWidth: '500px',
        padding: '32px',
        borderRadius: 'var(--radius-lg)',
        display: 'flex',
        flexDirection: 'column',
        gap: '24px',
        maxHeight: '80vh',
        overflowY: 'auto'
      }} onClick={e => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h2 style={{ fontSize: '1.5rem', fontWeight: 700, margin: 0 }}>System Settings</h2>
          <button onClick={onClose} style={{ 
            background: 'transparent', 
            border: 'none', 
            color: 'var(--text-dim)', 
            cursor: 'pointer',
            fontSize: '1.2rem'
          }}>✕</button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
          {/* Model Selection */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <label style={{ fontSize: '0.9rem', color: 'var(--text-dim)', fontWeight: 600 }}>Ollama Model</label>
            <input 
              type="text" 
              value={localSettings.ollama_model as string} 
              onChange={e => updateField('ollama_model', e.target.value)}
              className="sleek-card"
              style={{
                padding: '12px 16px',
                color: 'var(--text-main)',
                outline: 'none',
                background: 'rgba(255, 255, 255, 0.03)'
              }}
            />
          </div>

          {/* Temperature */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <label style={{ fontSize: '0.9rem', color: 'var(--text-dim)', fontWeight: 600 }}>Temperature</label>
              <span style={{ fontSize: '0.9rem', color: 'var(--primary)', fontWeight: 700 }}>{localSettings.temperature}</span>
            </div>
            <input 
              type="range" 
              min="0" 
              max="2" 
              step="0.1"
              value={localSettings.temperature} 
              onChange={e => updateField('temperature', parseFloat(e.target.value))}
              style={{ accentColor: 'var(--primary)', cursor: 'pointer' }}
            />
            <p style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>Lower is more focused/logical, higher is more creative.</p>
          </div>

          {/* Top P */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <label style={{ fontSize: '0.9rem', color: 'var(--text-dim)', fontWeight: 600 }}>Top P</label>
              <span style={{ fontSize: '0.9rem', color: 'var(--primary)', fontWeight: 700 }}>{localSettings.top_p}</span>
            </div>
            <input 
              type="range" 
              min="0" 
              max="1" 
              step="0.05"
              value={localSettings.top_p} 
              onChange={e => updateField('top_p', parseFloat(e.target.value))}
              style={{ accentColor: 'var(--primary)', cursor: 'pointer' }}
            />
          </div>

          {/* Log Level */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <label style={{ fontSize: '0.9rem', color: 'var(--text-dim)', fontWeight: 600 }}>Log Level</label>
            <select 
              value={localSettings.log_level} 
              onChange={e => updateField('log_level', parseInt(e.target.value))}
              className="sleek-card"
              style={{
                padding: '12px 16px',
                color: 'var(--text-main)',
                outline: 'none',
                background: 'rgba(255, 255, 255, 0.03)'
              }}
            >
              <option value={1}>Error</option>
              <option value={2}>Warn</option>
              <option value={3}>Info</option>
              <option value={4}>Debug</option>
              <option value={5}>Trace</option>
            </select>
          </div>
        </div>

        <button 
          onClick={handleSave}
          className="sleek-card"
          style={{
            marginTop: '12px',
            padding: '14px',
            background: 'var(--primary)',
            color: 'white',
            fontWeight: 700,
            border: 'none',
            cursor: 'pointer',
            boxShadow: '0 4px 15px var(--primary-glow)'
          }}
        >
          Save Changes
        </button>
      </div>
    </div>
  );
};

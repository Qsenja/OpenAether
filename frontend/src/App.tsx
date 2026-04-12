import { useBackend } from './hooks/useBackend';
import { Header } from './components/Header';
import { MessageList } from './components/MessageList';
import { InputBar } from './components/InputBar';
import { LoadingScreen } from './components/LoadingScreen';

function App() {
  const { messages, status, isConnected, isInitializing, sendMessage, stopGeneration, retry } = useBackend();

  if (!isConnected && isInitializing) {
    return <LoadingScreen onRetry={retry} />;
  }

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100vh',
      backgroundColor: 'var(--bg-dark)',
      position: 'relative'
    }}>
      <Header />
      
      <main style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        position: 'relative'
      }}>
        <MessageList messages={messages} status={status} />
      </main>

      <footer style={{
        paddingBottom: '20px'
      }}>
        <InputBar 
          onSendMessage={sendMessage} 
          onStop={stopGeneration} 
          status={status} 
          isConnected={isConnected}
        />
      </footer>
    </div>
  );
}

export default App;

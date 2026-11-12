import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { App } from './App';
import { Settings } from './settings/Settings';
import 'react-diff-view/style/index.css';
import './index.css';

// 데이터는 worktree-changed event로만 갱신한다 (Task 10)
const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false } },
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      {/* 설정창(label `settings`)과 main 창이 같은 bundle을 쓴다 */}
      {getCurrentWebviewWindow().label === 'settings' ? <Settings /> : <App />}
    </QueryClientProvider>
  </React.StrictMode>,
);

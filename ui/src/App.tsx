import { useQuery } from '@tanstack/react-query';
import { listRepos } from './api';

export function App() {
  const repos = useQuery({ queryKey: ['repos'], queryFn: listRepos });
  if (repos.error) return <pre className="whitespace-pre-wrap p-4 text-red-600">{String(repos.error)}</pre>;
  return <pre className="p-4">{JSON.stringify(repos.data, null, 2)}</pre>;
}

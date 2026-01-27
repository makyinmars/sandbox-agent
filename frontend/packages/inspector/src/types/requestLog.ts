export type RequestLog = {
  id: number;
  method: string;
  url: string;
  body?: string;
  status?: number;
  time: string;
  curl: string;
  error?: string;
};

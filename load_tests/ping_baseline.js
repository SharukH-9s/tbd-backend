import http from 'k6/http';
import { check } from 'k6';

export const options = {
  stages: [
    { duration: '10s', target: 50 },
    { duration: '30s', target: 50 },
    { duration: '10s', target: 0 },
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'],
  },
};

export default function () {
  const baseUrl = __ENV.BASE_URL || 'https://tbd-backend-gx8i.onrender.com';
  const res = http.get(`${baseUrl}/ping`);

  check(res, {
    'is status 200': (r) => r.status === 200,
    'is ok payload': (r) => r.json('status') === 'ok',
  });
}

import http from 'k6/http';
import { check } from 'k6';

export const options = {
  // Baseline test: measure server and network behavior without touching the database.
  stages: [
    { duration: '10s', target: 50 }, // Ramp up quickly to 50 simulated users.
    { duration: '30s', target: 50 }, // Hold the load steady to measure sustained behavior.
    { duration: '10s', target: 0 }, // Ramp back down to zero users.
  ],
  thresholds: {
    // Flag the test if the 95th percentile response time goes above 500ms.
    http_req_duration: ['p(95)<500'],
  },
};

export default function () {
  // Use the deployed backend unless a local URL is supplied through the environment.
  const baseUrl = __ENV.BASE_URL || 'https://tbd-backend-gx8i.onrender.com';
  // Hit the lightweight /ping endpoint so this test stays independent of the database.
  const res = http.get(`${baseUrl}/ping`);

  check(res, {
    // Confirm the endpoint is healthy.
    'is status 200': (r) => r.status === 200,
    // Confirm the response body contains the expected JSON payload.
    'is ok payload': (r) => r.json('status') === 'ok',
  });
}

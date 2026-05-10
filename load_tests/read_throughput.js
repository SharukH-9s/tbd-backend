import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  // Test 2: High-Throughput Read Test
  // Goal: See how many fast DB reads the CPU can handle before slowing down
  stages: [
    { duration: '10s', target: 50 },  // Ramp up to 50 users
    { duration: '30s', target: 100 }, // Hold at 100 users for 30s
    { duration: '10s', target: 0 },   // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'], // 95% of requests should be below 500ms
  },
};

export default function () {
  // Replace this with your actual deployed URL when testing against production
  const url = 'http://localhost:6751/turfs'; 
  
  const res = http.get(url);

  check(res, {
    'is status 200': (r) => r.status === 200,
  });

  // Short sleep to simulate real user behavior between clicks, but keep it low for high throughput
  sleep(0.1); 
}

import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  // Test 1: The "Slow Query" Pile-up
  // Goal: Saturate the database connection pool with heavy writes
  stages: [
    { duration: '15s', target: 100 }, // Ramp up quickly to 100 concurrent writers
    { duration: '30s', target: 300 }, // Push to 300 users to saturate the pool
    { duration: '15s', target: 0 },   // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<2000'], // We expect writes to take longer, but flag if p95 > 2s
  },
};

export default function () {
  const url = 'http://localhost:6751/register';
  
  // Generate random data for registration to ensure unique DB inserts
  const randomStr = Math.random().toString(36).substring(7);
  const payload = JSON.stringify({
    username: `k6user_${randomStr}`,
    email: `k6user_${randomStr}@example.com`,
    password: "Password123!",
    phone: `+1234567${Math.floor(Math.random() * 1000)}`
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
  };

  const res = http.post(url, payload, params);

  // Status 200/201 means success, 503 means our new acquire_timeout kicked in!
  check(res, {
    'is status success': (r) => r.status === 200 || r.status === 201,
    'is status 503 (timeout)': (r) => r.status === 503,
  });

  // Small sleep to control the loop rate
  sleep(1);
}

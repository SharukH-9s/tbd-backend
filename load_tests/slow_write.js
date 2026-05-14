import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  // Heavy write test: stress the database connection pool with repeated registrations.
  stages: [
    { duration: '15s', target: 100 }, // Ramp up quickly to 100 concurrent writers.
    { duration: '30s', target: 300 }, // Push harder to saturate the pool.
    { duration: '15s', target: 0 },   // Ramp back down to zero users.
  ],
  thresholds: {
    // Allow slower writes, but flag the run if the 95th percentile exceeds 2 seconds.
    http_req_duration: ['p(95)<2000'],
  },
};

export default function () {
  // Use the deployed backend unless a local URL is supplied through the environment.
  const url = 'https://tbd-backend-gx8i.onrender.com/register';
  
  // Generate random data so each registration attempt is unique.
  const randomStr = Math.random().toString(36).substring(7);
  const payload = JSON.stringify({
    username: `k6user_${randomStr}`,
    email: `k6user_${randomStr}@example.com`,
    password: 'Password123!',
    phone: `+1234567${Math.floor(Math.random() * 1000)}`
  });

  // Tell the backend that the request body is JSON.
  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
  };

  // Send the registration request and capture the response.
  const res = http.post(url, payload, params);

  // Treat successful registration as a pass; 503 indicates the DB pool timed out.
  check(res, {
    'is status success': (r) => r.status === 200 || r.status === 201,
    'is status 503 (timeout)': (r) => r.status === 503,
  });

  // Add a small pause to avoid a zero-think-time flood of writes.
  sleep(1);
}

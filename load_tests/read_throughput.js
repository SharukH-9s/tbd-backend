import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  // Test 2: High-Throughput Read Test
  // Goal: See how many fast DB reads the CPU can handle before slowing down
  stages: [
    { duration: `${__ENV.RAMP_UP_DURATION || '10s'}`, target: Number(__ENV.RAMP_UP_USERS || 50) },
    { duration: `${__ENV.HOLD_DURATION || '30s'}`, target: Number(__ENV.HOLD_USERS || 100) },
    { duration: `${__ENV.RAMP_DOWN_DURATION || '10s'}`, target: 0 },
  ],
  thresholds: {
    http_req_duration: [`p(95)<${Number(__ENV.P95_THRESHOLD_MS || 500)}`],
  },
};

function requireEnv(primaryName, fallbackName) {
  const primaryValue = __ENV[primaryName];
  if (primaryValue) {
    return primaryValue;
  }

  const fallbackValue = __ENV[fallbackName];
  if (fallbackValue) {
    return fallbackValue;
  }

  throw new Error(`Missing required environment variable: ${primaryName} or ${fallbackName}`);
}

export function setup() {
  const baseUrl = __ENV.BASE_URL || 'https://tbd-backend-gx8i.onrender.com';
  const email = requireEnv('LOGIN_EMAIL', 'ADMIN_EMAIL');
  const password = requireEnv('LOGIN_PASSWORD', 'ADMIN_PASSWORD');

  const loginResponse = http.post(
    `${baseUrl}/login`,
    JSON.stringify({ email, password }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(loginResponse, {
    'login returned 200': (r) => r.status === 200,
  });

  if (loginResponse.status !== 200) {
    throw new Error(`Login failed with status ${loginResponse.status}: ${loginResponse.body}`);
  }

  const loginBody = JSON.parse(loginResponse.body);
  const accessToken = loginBody?.data?.access_token;

  if (!accessToken) {
    throw new Error(`Login response did not include data.access_token: ${loginResponse.body}`);
  }

  return { baseUrl, accessToken };
}

export default function (data) {
  const res = http.get(`${data.baseUrl}/turfs`, {
    headers: {
      Authorization: `Bearer ${data.accessToken}`,
    },
  });

  check(res, {
    'is status 200': (r) => r.status === 200,
  });

  // Simulate real user behavior - wait 1 second between requests
  sleep(1); 
}

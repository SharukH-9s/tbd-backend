import http from 'k6/http'; // is a module provided by k6 for making HTTP requests in our test scripts
import { check, sleep } from 'k6'; // check is a function that allows us to define assertions on our HTTP responses, and sleep is a function that lets us pause execution for a specified duration, simulating real user think time

export const options = { // options is a Configuration Object and use export const to make it available to k6
  // Test 2: High-Throughput Read Test
  // Goal: See how many fast DB reads the CPU can handle before slowing down
  stages: [ // is a key of options object containing an array of stages to simulate different load patterns
            // we can adjust these via environment variables for flexibility  
    { duration: `${__ENV.RAMP_UP_DURATION || '10s'}`, target: Number(__ENV.RAMP_UP_USERS || 50) },  // Ramp up to a certain number of virtual users (VUs) over a specified duration. This simulates a gradual increase in load on the system. here k6 is adding 5 users every second
    { duration: `${__ENV.HOLD_DURATION || '30s'}`, target: Number(__ENV.HOLD_USERS || 100) },       // Hold the load at a certain number of VUs for a specified duration. This simulates a sustained load on the system, allowing us to observe how it performs under consistent traffic. here k6 is maintaining 100 users for 30 seconds
    { duration: `${__ENV.RAMP_DOWN_DURATION || '10s'}`, target: 0 },                                // Ramp down to 0 VUs over a specified duration  
  ],
  thresholds: { // is a key of options object, and is an object itself.
                // containing performance thresholds that k6 will check against the test results
    http_req_duration: [`p(95)<${Number(__ENV.P95_THRESHOLD_MS || 500)}`], // key of thresholds object, and is an array of strings defining the threshold condition.
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

  const loginResponse = http.post( // loginResponse is the whole k6 HTTP response object returned by http.post(...). It contains things like: timings, status, body, headers, cookies, etc.

    `${baseUrl}/login`, // 1. Where to go
    JSON.stringify({ email, password }), // 2. What to send (the Body of the request)
    { headers: { 'Content-Type': 'application/json' } } // 3. Any additional options, like headers (here we specify that we're sending JSON data)
  );

  check(loginResponse, {
    'login returned 200': (r) => r.status === 200,}); // the {} is an object where The Key (The Label) is '....' part. The Value is a function that takes the response (r) and returns true if the condition is met (in this case, if the status code is 200)

  if (loginResponse.status !== 200) {
    throw new Error(`Login failed with status ${loginResponse.status}: ${loginResponse.body}`);
  }

  const loginBody = JSON.parse(loginResponse.body); // loginResponse.body is a string. We need to parse it into a JavaScript object to access its properties. loginBody is the parsed JSON object from the login response, which should contain the access token we need for authenticated requests.
  const accessToken = loginBody?.data?.access_token; // We use optional chaining (?.) to safely access nested properties. If any part of the chain is undefined or null, accessToken will be set to undefined instead of throwing an error. This allows us to handle cases where the expected structure of the response might not be present, such as if the login failed or the response format changed.

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

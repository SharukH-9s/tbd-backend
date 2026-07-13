const https = require('https');

function makeRequest(index) {
  return new Promise((resolve) => {
    const options = {
      hostname: 'tbd-backend-gx8i.onrender.com',
      port: 443,
      path: '/login',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      }
    };

    const req = https.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => { data += chunk; });
      res.on('end', () => {
        console.log(`[Request ${index}] Status: ${res.statusCode}`);
        console.log(`             Limit: ${res.headers['x-ratelimit-limit']}`);
        console.log(`             Remaining: ${res.headers['x-ratelimit-remaining']}`);
        console.log(`             Retry-After: ${res.headers['retry-after']}`);
        resolve();
      });
    });

    req.on('error', (e) => {
      console.error(`Request ${index} failed:`, e);
      resolve();
    });

    // Send empty payload (causes 400 Bad Request, but counts against the rate limit)
    req.write(JSON.stringify({}));
    req.end();
  });
}

async function run() {
  console.log("Starting rate limiter test against: https://tbd-backend-gx8i.onrender.com/login");
  for (let i = 1; i <= 5; i++) {
    await makeRequest(i);
  }
}

run();

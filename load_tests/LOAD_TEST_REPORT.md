# Load Testing Report

## Executive Summary

Baseline and throughput testing reveals that with **Neon Free tier (~20 max connections)**, the application can reliably handle **~15 concurrent users** at acceptable performance levels (p(95) < 500ms).

---

## Test Environment

- **Backend:** Render (Railway equivalent)
- **Database:** Neon PostgreSQL (Free tier)
- **Max DB Connections:** ~20
- **Load Test Tool:** k6
- **Tested Endpoint:** `/turfs` (authenticated GET)

---

## Test Results

### A. No-DB Baseline (Ping Endpoint)

**Objective:** Establish Render infrastructure limits without database dependency.

**Configuration:**
- Endpoint: `/ping` (static `{"status": "ok"}`)
- VUs: 50
- Duration: 50s
- Sleep: 0s (no think time)

**Results:**
```
Throughput: 166 req/s
p(95): 263.47ms ✓
p(90): 260.07ms ✓
Failures: 0%
```

**Conclusion:** Render infrastructure is solid. Bottleneck is NOT the server/network.

---

### B. Connection Pool Testing: 100 VUs (pool=50)

**Objective:** Test at high load with increased pool size.

**Configuration:**
- VUs: 100
- Max Connections: 50
- Sleep: 1s (realistic user behavior)
- Duration: 50s

**Results:**
```
Throughput: 3.6 req/s ✗ (expected ~100)
p(95): 33.8s ✗ (target: <500ms)
Failures: 88.44%
Avg Response: 21.59s
```

**Conclusion:** **Database connection pool exhaustion.** Neon Free tier (~20 conn limit) unable to support 100 VUs even with max_connections=50 configured.

---

### C. Connection Pool Testing: 25 VUs (pool=20)

**Objective:** Test at moderate load within Neon Free tier limits.

**Configuration:**
- VUs: 25
- Max Connections: 20
- Sleep: 1s
- Duration: 50s

**Results:**
```
Throughput: 12.5 req/s
p(95): 1.42s ✗ (target: <500ms)
p(90): 408ms ✓
Failures: 0.59%
Avg Response: 522ms
```

**Conclusion:** 25 VUs begins stressing the connection pool. p(95) exceeds threshold due to occasional queue waits.

---

### D. Connection Pool Testing: 15 VUs (pool=20)

**Objective:** Find the stable operating point within Neon Free tier.

**Configuration:**
- VUs: 15
- Max Connections: 20
- Sleep: 1s
- Duration: 50s

**Results:**
```
Throughput: 8.4 req/s
p(95): 397.64ms ✓ (PASSED)
p(90): 389.76ms ✓
Failures: 0%
Avg Response: 379.91ms
Max Response: 2.44s
```

**Conclusion:** **15 VUs is the stable operating point.** All requests complete within acceptable latency, no failures.

---

## Key Findings

| Metric      15 VUs         25 VUs              100 VUs 

| Throughput  8.4 req/s     12.5 req/s          3.6 req/s 
| p(95)       **397ms ✓**   1.42s ✗            33.8s ✗ 
| p(90)        389.76ms ✓   408ms ✓            33.3s ✗ 
| Failures     0%           0.59%               88.44% 
| Avg Response 379.91ms     522ms               21.59s 
| **Status** | **STABLE** | **DEGRADED** | **FAILED** |

---

## Root Cause Analysis

**The Bottleneck:** Database Connection Pool

1. **Neon Free tier limitation:** ~20 concurrent connections max
2. **Current config:** `max_connections(20)` matches Neon limit
3. **At 15 VUs:** Connection pool has sufficient headroom
4. **At 25+ VUs:** Request queue builds, p(95) degradation begins
5. **At 100 VUs:** Pool exhaustion, 88% timeouts/failures

**Why the free tier fails at scale:**
- 100 VUs with 1s sleep = ~100 concurrent requests needed
- Neon Free tier provides only ~20 connections
- Acquire timeout (3s) expires → requests fail

---

## Recommendations

### For Development/Testing
✓ **Current setup acceptable** for:
- Single user / small team testing
- Development environments
- Load testing up to 15 VUs

### For Production (Scale Beyond 15 VUs)

**Option 1: Upgrade Neon Plan (RECOMMENDED)**
- Cost: ~$15-30/month
- Benefit: 100+ connections, no code changes
- Effort: 5 minutes
- Impact: Immediate

**Option 2: Implement PgBouncer**
- Cost: Free (requires self-hosting)
- Benefit: Connection pooling/multiplexing
- Effort: Medium (requires infrastructure setup)
- Impact: Can support 100+ concurrent users

**Option 3: Optimize Database Queries**
- Cost: Free
- Benefit: Faster query execution = faster connection release
- Effort: High (requires profiling & optimization)
- Impact: Allows more VUs on same pool size

---

## How to Run Tests

### Baseline (No DB)
```bash
k6 run load_tests/ping_baseline.js
```

### Read Throughput (With DB)
```bash
# Set your credentials
$env:BASE_URL="https://tbd-backend-gx8i.onrender.com"
$env:LOGIN_EMAIL="your-email@example.com"
$env:LOGIN_PASSWORD="your-password"
$env:RAMP_UP_USERS="15"
$env:HOLD_USERS="15"

k6 run load_tests/read_throughput.js
```

### Write Test (Registration)
```bash
$env:BASE_URL="https://tbd-backend-gx8i.onrender.com"
k6 run load_tests/slow_write.js
```

---

## Test Methodology

- **Load Ramp:** 10s ramp-up to target VUs, 30s hold, 10s ramp-down
- **Think Time:** 1s between requests (simulates realistic user behavior)
- **Failure Threshold:** 3s connection acquire timeout
- **Success Threshold:** p(95) < 500ms

---

## Next Steps

1. **Immediate:** Document findings (this report) ✓
2. **Short-term:** Either upgrade Neon or implement PgBouncer
3. **Long-term:** Profile slow queries and optimize database performance
4. **Monitoring:** Set up alerts for p(95) > 500ms in production

---

## Appendix: Test Configuration

### Database Pool Settings
```rust
.max_connections(20)              // Neon Free tier limit
.acquire_timeout(Duration::from_secs(3))  // Fail fast if exhausted
```

### Load Test Parameters
```javascript
stages: [
  { duration: '10s', target: 15 },  // Ramp up
  { duration: '30s', target: 15 },  // Hold
  { duration: '10s', target: 0 },   // Ramp down
]
sleep(1);  // 1 second between requests
thresholds: { http_req_duration: ['p(95)<500'] }
```

---

**Report Generated:** 2026-05-12
**Tested by:** Load Test Analysis
**Status:** Complete

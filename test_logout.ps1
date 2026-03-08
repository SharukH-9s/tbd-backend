$baseUrl = "http://localhost:6751"
$loginEndpoint = "$baseUrl/login"
$logoutEndpoint = "$baseUrl/logout"

# 1. Login first to get a refresh token cookie
$loginBody = @{
    email    = "test_173057@example.com"
    password = "SafePassword123"
} | ConvertTo-Json

Write-Host "--- Step 1: Logging in to get session ---" -ForegroundColor Cyan
$session = New-Object Microsoft.PowerShell.Commands.WebRequestSession
try {
    $loginRes = Invoke-RestMethod -Uri $loginEndpoint -Method Post -Body $loginBody -ContentType "application/json" -WebSession $session
    Write-Host "Login Success!" -ForegroundColor Green
    
    # Check if we have cookies
    $cookie = $session.Cookies.GetCookies($baseUrl) | Where-Object { $_.Name -eq "refresh_token" }
    if ($cookie) {
        Write-Host "Refresh Token Cookie found: $($cookie.Value.Substring(0, 10))..." -ForegroundColor Yellow
    } else {
        Write-Host "Warning: No refresh_token cookie found in session!" -ForegroundColor Red
    }

    # 2. Perform Logout
    Write-Host "`n--- Step 2: Logging out ---" -ForegroundColor Cyan
    $logoutRes = Invoke-RestMethod -Uri $logoutEndpoint -Method Post -WebSession $session
    Write-Host "Logout Success!" -ForegroundColor Green
    $logoutRes | Format-List

    # 3. Verify cookie is cleared
    $cookieAfter = $session.Cookies.GetCookies($baseUrl) | Where-Object { $_.Name -eq "refresh_token" }
    if ($cookieAfter -and ($cookieAfter.Expired -or $cookieAfter.Value -eq "")) {
        Write-Host "Cookie successfully cleared/expired." -ForegroundColor Green
    } else {
        Write-Host "Note: Session object might still hold the cookie object, checking database is the ultimate test." -ForegroundColor Gray
    }

} catch {
    Write-Host "Test Failed!" -ForegroundColor Red
    $_.Exception.Message
}

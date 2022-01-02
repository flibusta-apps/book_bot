(async () => {
    const http = await import('http');

    const healthCheck = http.request("http://localhost:8080/healthcheck", (res) => {
        console.log(`HEALTHCHECK STATUS: ${res.statusCode}`);
        if (res.statusCode == 200) {
            process.exit(0);
        }
        else {
            process.exit(1);
        }
    });

    healthCheck.on('error', function (err) {
        console.error('ERROR');
        process.exit(1);
    });

    healthCheck.end();
})();

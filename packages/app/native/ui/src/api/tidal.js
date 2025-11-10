window.api = {
    async startTidalAuth() {
        try {
            const response = await fetch('/api/tidal/auth', {
                method: 'POST',
            });
            if (!response.ok) {
                throw new Error('Failed to start Tidal authentication');
            }
            const url = await response.text();
            window.open(url, '_blank');
        } catch (error) {
            console.error('Error starting Tidal authentication:', error);
        }
    },

    async runTidalScan() {
        try {
            const button = document.getElementById('run-scan-button');
            button.disabled = true;
            const response = await fetch('/api/tidal/scan', {
                method: 'POST',
            });
            if (!response.ok) {
                throw new Error('Failed to run Tidal scan');
            }
            button.disabled = false;
        } catch (error) {
            console.error('Error running Tidal scan:', error);
            const button = document.getElementById('run-scan-button');
            button.disabled = false;
        }
    },
};

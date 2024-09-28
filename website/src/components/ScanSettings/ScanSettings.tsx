import './ScanSettings.css';
import { api } from '~/services/api';

export default function scanSettingsRender() {
    return (
        <div class="scan-settings-container">
            <button
                onClick={async () => api.startScan(['LOCAL'])}
                type="button"
                class="remove-button-styles moosicbox-button"
            >
                Scan
            </button>
        </div>
    );
}

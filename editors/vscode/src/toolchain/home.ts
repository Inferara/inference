import * as os from 'os';
import * as path from 'path';

/** Resolve the INFERENCE_HOME directory (default: ~/.inference). */
export function inferenceHome(): string {
    return process.env['INFERENCE_HOME'] || path.join(os.homedir(), '.inference');
}

// Main entry point for the example JavaScript application
import { greet } from './utils.js';
import { calculate } from './math.js';

function main() {
    console.log(greet('HyperChad'));
    console.log(`Calculation result: ${calculate(10, 5)}`);
    console.log('Bundling example complete!');
}

main();

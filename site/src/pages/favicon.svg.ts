// Serves /favicon.svg from the brand folder so #687 swaps one file and nothing else.
import svg from '../assets/brand/concept-2-16.svg?raw';

export function GET() {
  return new Response(svg, { headers: { 'Content-Type': 'image/svg+xml' } });
}

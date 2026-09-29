/**
 * Shop: a tiny cart library used to test figure on TypeScript.
 */

import { Cart } from './store/cart';
import { render } from './ui/view';

/** Builds a cart and prints it. */
export function main(): void {
  render(new Cart());
}

export { Cart } from './store/cart';
export * from './store/types';

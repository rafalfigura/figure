/**
 * Rendering: turns carts into text and React elements.
 */

import React from 'react';
import type { Cart } from '@/store/cart';

/** Formats an amount with its currency. */
export function format(amount: number, currency: string): string {
  return `${amount.toFixed(2)} ${currency}`;
}

/** Renders the cart total. */
export function render(cart: Cart): React.ReactElement {
  return <span>{format(cart.total(), 'EUR')}</span>;
}

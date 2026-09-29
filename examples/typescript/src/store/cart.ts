/**
 * The cart: items in, total out.
 */

import { Item, Status, DEFAULT_CURRENCY } from './types';
import { format } from '../ui/view';

/** Holds items until they are paid. */
export class Cart implements Iterable<Item> {
  status: Status = Status.Open;
  private items: Item[] = [];

  /** Adds an item. */
  add(item: Item): void {
    this.items.push(item);
  }

  /** Sum of all prices. */
  total(): number {
    return this.sum();
  }

  private sum(): number {
    return this.items.reduce((n, i) => n + i.price, 0);
  }

  /** Iterates the items. */
  [Symbol.iterator](): Iterator<Item> {
    return this.items[Symbol.iterator]();
  }
}

/** Formats a cart for humans. */
export const summary = (cart: Cart, currency = DEFAULT_CURRENCY): string =>
  format(cart.total(), currency);

function unexported() {}

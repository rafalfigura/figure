/**
 * Item and price types shared by the store.
 */

/** A purchasable thing. */
export interface Item {
  id: string;
  price: number;
  label?: string;
  describe(): string;
}

/** Where a price is counted. */
export type Currency = 'EUR' | 'USD';

/** Cart states. */
export enum Status {
  Open,
  Paid,
}

/** Default currency. */
export const DEFAULT_CURRENCY: Currency = 'EUR';

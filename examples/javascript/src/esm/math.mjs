/**
 * Math helpers written as an ES module.
 */

/** Adds two numbers. */
export const add = (a, b) => a + b;

/** Counts things. */
export class Counter {
  count = 0;

  /** Increments and returns the count. */
  bump() {
    return ++this.count;
  }
}

function internal() {}

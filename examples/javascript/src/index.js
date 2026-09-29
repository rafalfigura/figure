/**
 * Toolbox: a small JavaScript library that mixes ES modules and CommonJS.
 */

import { add, Counter } from './esm/math.mjs';
import legacy from './cjs/legacy.cjs';
import _ from 'lodash';

/** Runs both halves and returns the total. */
export default function run() {
  const counter = new Counter();
  counter.bump();
  return _.sum([add(1, 2), legacy.double(3)]);
}

export { slug } from './cjs/legacy.cjs';
export * from './esm/math.mjs';

/**
 * Older helpers written as a CommonJS module.
 */

const { add } = require('../esm/math.mjs');

/** Doubles a number. */
function double(n) {
  return n * 2;
}

/** Lower-cases and dashes a title. */
exports.slug = (title) => title.toLowerCase().replace(/ /g, '-');

function hidden() {}

module.exports = { double, add };

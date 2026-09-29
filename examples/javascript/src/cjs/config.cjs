/**
 * Configuration as a single exported class.
 */

/** Settings read from the environment. */
module.exports = class Config {
  /** Reads one setting. */
  get(key) {
    return process.env[key];
  }
};

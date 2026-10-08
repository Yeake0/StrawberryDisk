const SCALE_TRANSITION_MS = 600;

/** Buckets rate ranges and eases their amplitude without rebuilding paths per frame. */
export class ResourceTrendScale {
  ceiling = 1;
  private fromInverse = 1;
  private changedAt = -Infinity;
  private holdUntil = 0;
  private initialized = false;

  amplitude(clock: number): number {
    const progress = Math.max(0, Math.min(1, (clock - this.changedAt) / SCALE_TRANSITION_MS));
    const eased = progress * progress * (3 - 2 * progress);
    // Interpolate the reciprocal: it is the line's visual height multiplier.
    // Retargeting from the current multiplier preserves continuity mid-transition.
    return this.ceiling * (this.fromInverse * (1 - eased) + eased / this.ceiling);
  }

  update(peak: number, clock: number, immediate = false) {
    const target = 2 ** Math.ceil(Math.log2(Math.max(1, peak)));
    if (!this.initialized || peak > this.ceiling || (peak < this.ceiling / 4 && clock >= this.holdUntil)) {
      this.fromInverse = this.amplitude(clock) / this.ceiling;
      this.ceiling = target;
      this.changedAt = clock;
      this.holdUntil = clock + 30_000;
      if (!this.initialized) immediate = true;
      this.initialized = true;
    }
    if (immediate) this.finish();
  }

  finish() {
    this.fromInverse = 1 / this.ceiling;
    this.changedAt = -Infinity;
  }

  reset() {
    this.initialized = false;
  }
}

export interface FrameSample {
  time: number;
  frames: number;
  callbacks: number;
}

export function frameRates(previous: FrameSample, current: FrameSample) {
  const seconds = (current.time - previous.time) / 1000;

  if (
    seconds <= 0 ||
    current.frames < previous.frames ||
    current.callbacks < previous.callbacks
  )
    return null;

  return {
    submitted: (current.frames - previous.frames) / seconds,
    callbacks: (current.callbacks - previous.callbacks) / seconds,
  };
}

export function seekPosition(value: number, duration: number) {
  if (!Number.isFinite(value) || !Number.isFinite(duration) || duration <= 0)
    return null;

  return Math.max(0, Math.min(duration, value));
}

export function bufferedSeconds(
  ranges: {
    length: number;
    start: (index: number) => number;
    end: (index: number) => number;
  },
  position: number,
) {
  for (let index = 0; index < ranges.length; index++) {
    if (ranges.start(index) <= position && ranges.end(index) >= position)
      return Math.max(0, ranges.end(index) - position);
  }

  return 0;
}

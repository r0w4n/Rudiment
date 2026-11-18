import { writeFileSync } from 'node:fs';

const sampleRate = 44_100;
let seed = 0x5eed1234;
const noise = () => {
  seed = (1664525 * seed + 1013904223) >>> 0;
  return seed / 0x80000000 - 1;
};

function wav(name, seconds, sample) {
  const count = Math.round(seconds * sampleRate);
  const bytes = Buffer.alloc(44 + count * 2);
  bytes.write('RIFF', 0);
  bytes.writeUInt32LE(36 + count * 2, 4);
  bytes.write('WAVEfmt ', 8);
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(sampleRate, 24);
  bytes.writeUInt32LE(sampleRate * 2, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36);
  bytes.writeUInt32LE(count * 2, 40);
  for (let index = 0; index < count; index++) {
    const value = Math.max(-1, Math.min(1, sample(index / sampleRate)));
    bytes.writeInt16LE(Math.round(value * 32767), 44 + index * 2);
  }
  writeFileSync(new URL(`../web/public/audio/${name}.wav`, import.meta.url), bytes);
}

wav('kick', .48, time => {
  const envelope = Math.exp(-time * 9.5);
  const phase = 2 * Math.PI * (43 * time + 38 * (1 - Math.exp(-time * 25)) / 25);
  const attack = time < .012 ? noise() * (1 - time / .012) * .2 : 0;
  return Math.tanh((Math.sin(phase) * envelope + attack) * 1.35) * .82;
});

function hatSample(decay) {
  let previous = 0;
  return time => {
    const current = noise();
    const high = current - previous * .88;
    previous = current;
    const metallic = Math.sin(2 * Math.PI * 6173 * time) * .16
      + Math.sin(2 * Math.PI * 8431 * time) * .11;
    return (high * .42 + metallic) * Math.exp(-time * decay) * .72;
  };
}

wav('closed-hat', .13, hatSample(34));
wav('open-hat', .92, hatSample(4.8));

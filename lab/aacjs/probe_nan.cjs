// TASK-20 lab probe: instrument AAC.js to find where NaNs / parse divergence start.
// Usage: node probe_nan.cjs <input.adts>
'use strict';
const AV = require('av');
const ICS = require('aac/src/ics');

let frameNo = 0;
let logged = false;

const origSpec = ICS.prototype.decodeSpectralData;
ICS.prototype.decodeSpectralData = function (stream) {
  origSpec.call(this, stream);
  if (logged) return;
  const d = this.data;
  for (let i = 0; i < d.length; i++) {
    if (Number.isNaN(d[i])) {
      logged = true;
      console.log('FIRST_NAN frame', frameNo, 'specIndex', i,
        'bandTypes', Array.from(this.bandTypes.slice(0, 12)),
        'scaleFactors', Array.from(this.scaleFactors.slice(0, 12)),
        'sfHasNaN', Array.from(this.scaleFactors).some(Number.isNaN));
      break;
    }
  }
};

const origInfoDecode = ICS.prototype.decode;
ICS.prototype.decode = function (stream, config, commonWindow) {
  origInfoDecode.call(this, stream, config, commonWindow);
  if (frameNo < 6) {
    console.log('frame', frameNo, 'winSeq', this.info.windowSequence,
      'maxSFB', this.info.maxSFB, 'groups', this.info.groupCount);
  }
};

require('aac');

const asset = AV.Asset.fromFile(process.argv[2]);
asset.on('format', (f) => console.log('format', f.sampleRate, f.channelsPerFrame + 'ch'));
asset.on('error', (e) => console.log('ERROR', String(e && e.message || e)));
let chunks = 0;
asset.on('data', () => { chunks++; frameNo++; });
asset.on('end', () => { console.log('end chunks', chunks); process.exit(0); });
asset.start(true);
setTimeout(() => { console.log('timeout chunks', chunks); process.exit(3); }, 20000);

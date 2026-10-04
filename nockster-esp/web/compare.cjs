const bs58 = require('bs58');

const web = '3wAKMVdgXFQ2iYjHxiz4KZPMh353qwcZYtTpfMtGqUDREsXxHNo174n1GwRPQ5RSmPt4WxQeLrvb2RkSbMuMYPBeFN2CF434Vguc6ZcFh3PxjiudznqGqa3HmF8uQeNMJcmC';
const expected = '32bePYRuJ3heGVEbznc6xSCaTymgz9bGFREaZ2dtJdnepjc6RX7cMSP8ATeT8bHTfxFmS7StDTmFHfvt9GP1PUq99pN7DcEFat9SDBpQwJbnwmhn5JHcGpLsRKp4fxfHSRy5';

const webBytes = bs58.decode(web);
const expBytes = bs58.decode(expected);

console.log('Web serialized:', webBytes.toString('hex'));
console.log('\nExpected:      ', expBytes.toString('hex'));

let firstDiff = -1;
for (let i = 0; i < 97; i++) {
  if (webBytes[i] !== expBytes[i]) {
    firstDiff = i;
    break;
  }
}
console.log('\nFirst difference at byte:', firstDiff);
if (firstDiff >= 0) {
  const web_hex = webBytes[firstDiff].toString(16).padStart(2, '0');
  const exp_hex = expBytes[firstDiff].toString(16).padStart(2, '0');
  console.log('  Web[' + firstDiff + '] = 0x' + web_hex);
  console.log('  Exp[' + firstDiff + '] = 0x' + exp_hex);
}

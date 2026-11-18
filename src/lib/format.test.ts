import { describe, expect, it } from 'vitest';
import { authorsShort, fmtBytes, splitMarkers } from './format';

describe('format', () => {
  it('splits citation markers', () => {
    expect(splitMarkers('결과가 향상되었다 [S1][S2]. 끝')).toEqual(['결과가 향상되었다 ', { label: 'S1' }, { label: 'S2' }, '. 끝']);
    expect(splitMarkers('표기 없음')).toEqual(['표기 없음']);
  });
  it('formats', () => {
    expect(fmtBytes(null)).toBe('확인 불가');
    expect(fmtBytes(2048)).toBe('2 KB');
    expect(authorsShort(['가', '나', '다'])).toBe('가 외 2명');
    expect(authorsShort([])).toBe('저자 정보 없음');
  });
});

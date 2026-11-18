export const fmtBytes = (n: number | null | undefined) => {
  if (n == null) return '확인 불가';
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
};
export const fmtDate = (s: string) => {
  try {
    const d = new Date(s);
    return `${d.getFullYear()}.${String(d.getMonth() + 1).padStart(2, '0')}.${String(d.getDate()).padStart(2, '0')}`;
  } catch {
    return s;
  }
};
export const authorsShort = (a: string[]) => (a.length === 0 ? '저자 정보 없음' : a.length <= 2 ? a.join(', ') : `${a[0]} 외 ${a.length - 1}명`);
export const STATUS_LABEL: Record<string, string> = {
  ok: '본문 추출 완료', partial: '일부 쪽 글자 없음', no_text: '글자 없음 (OCR 필요)', failed: '본문 추출 실패', metadata_only: '서지정보만', pending: '처리 대기',
};
export const SOURCE_LABEL: Record<string, string> = {
  user: '직접 입력', pdf_info: 'PDF 속성', text: '본문 표기', text_guess: '자동 추정 — 확인 필요', file_name: '파일 이름', bibtex: 'BibTeX', ris: 'RIS', demo: '가상 데모 자료',
};
/** [S1] 표기를 분리해 렌더링하기 위한 토큰화 */
export function splitMarkers(text: string): (string | { label: string })[] {
  const out: (string | { label: string })[] = [];
  const re = /\[(S\d+)\]/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push({ label: m[1] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

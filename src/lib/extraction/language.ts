const codes: Record<string, string> = {
  fra: 'fr',
  fre: 'fr',
  eng: 'en',
  jpn: 'ja',
  deu: 'de',
  ger: 'de',
  spa: 'es',
  ita: 'it',
  por: 'pt',
  zho: 'zh',
  chi: 'zh',
  kor: 'ko',
  rus: 'ru',
  nld: 'nl',
  dut: 'nl',
};
export function languageName(code: string | null | undefined): string {
  if (!code || code === 'und') return 'Unknown language';
  try {
    return (
      new Intl.DisplayNames(['en'], { type: 'language' }).of(
        codes[code] ?? code,
      ) ?? code
    );
  } catch {
    return code;
  }
}

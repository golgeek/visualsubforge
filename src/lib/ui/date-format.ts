// Set once before mounting the workspace. Browser-only views use the fixed fallback.
let formatter: Intl.DateTimeFormat | undefined;

export function setDateRegion(region: string | null) {
  formatter = undefined;
  if (!region || !/^(?:[a-z]{2}|[0-9]{3})$/i.test(region)) return;
  const locale = 'en-' + region.toUpperCase();
  const regional = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
  if (regional.resolvedOptions().locale !== locale) return;

  // Some English regional patterns (e.g. en-MV) use digits even for a medium month.
  const month = regional.formatToParts(0).find((p) => p.type === 'month');
  formatter =
    month && /[a-z]/i.test(month.value)
      ? regional
      : new Intl.DateTimeFormat(locale, {
          day: 'numeric',
          month: 'short',
          year: 'numeric',
          hour: 'numeric',
          minute: '2-digit',
        });
}

const months = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

export function formatDateTime(ms: number): string {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return 'Invalid Date';
  if (formatter) return formatter.format(date);
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${date.getDate()} ${months[date.getMonth()]} ${date.getFullYear()}, ${hh}:${mm}`;
}

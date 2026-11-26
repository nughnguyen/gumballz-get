export const DONATE_CONFIG = {
  zypageUrl: "https://zypage.com/gumballz",
  bank: "OCB",
  account: "0388205003",
  accountName: "NGUYEN QUOC HUNG",
  memo: "Ung Ho GumballZ Get",
  qrBase: "https://img.vietqr.io/image/OCB-0388205003-payos.svg",
} as const;

export function vietQrUrl(amount: string, note: string): string {
  const params = new URLSearchParams();
  if (amount) params.set("amount", amount.replace(/\D/g, ""));
  params.set("addInfo", note);
  params.set("accountName", DONATE_CONFIG.accountName);
  return `${DONATE_CONFIG.qrBase}?${params.toString()}`;
}

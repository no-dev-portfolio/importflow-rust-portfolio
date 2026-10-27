export function createEpoch(){
 let value=0;
 /** @param {number} ticket */
 const isCurrent=(ticket)=>ticket===value;
 return {next:()=>++value,current:()=>value,isCurrent};
}
/** @param {string} value */
export function yen(value){return new Intl.NumberFormat('ja-JP',{style:'currency',currency:'JPY',maximumFractionDigits:0}).format(BigInt(value));}

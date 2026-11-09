declare module "nspell" {
  interface Spell {
    correct(word: string): boolean;
    suggest(word: string): string[];
    add(word: string): Spell;
  }
  const nspell: (aff: string, dic: string) => Spell;
  export default nspell;
}

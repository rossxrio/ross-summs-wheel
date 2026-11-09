// Run with `npm test` (Node's built-in runner; Node strips the types).
import assert from "node:assert/strict";
import { test } from "node:test";
import { parseVoice, type VoiceEnemy } from "../src/voice/parse.ts";

const words = (text: string, agoMs = 0) => text.split(" ").map((t) => ({ text: t, agoMs }));
const timers = (text: string, enemies: VoiceEnemy[] = GAME) =>
  parseVoice(words(text), enemies).commands.map((c) => (c.kind === "timer" ? `${c.role} ${c.spell}` : c.kind));

const GAME: VoiceEnemy[] = [
  { role: "TOP", champion: "Darius", spells: ["SummonerFlash", "SummonerTeleport"] },
  { role: "JUNGLE", champion: "Lee Sin", spells: ["SummonerFlash", "SummonerSmite"] },
  { role: "MIDDLE", champion: "Ahri", spells: ["SummonerFlash", "SummonerDot"] },
  { role: "BOTTOM", champion: "Jinx", spells: ["SummonerFlash", "SummonerHeal"] },
  { role: "UTILITY", champion: "Thresh", spells: ["SummonerFlash", "SummonerExhaust"] },
];

test("role + spell", () => {
  assert.deepEqual(timers("ADC no flash"), ["BOTTOM SummonerFlash"]);
  assert.deepEqual(timers("Jungle, no smite."), ["JUNGLE SummonerSmite"]);
  assert.deepEqual(timers("top TP down"), ["TOP SummonerTeleport"]);
  assert.deepEqual(timers("Sup, exhaust."), ["UTILITY SummonerExhaust"]);
});

test("champion names, including two-word and misheard ones", () => {
  assert.deepEqual(timers("Jinx no flash"), ["BOTTOM SummonerFlash"]);
  assert.deepEqual(timers("Lee Sin flash"), ["JUNGLE SummonerFlash"]);
  assert.deepEqual(timers("Thrush no flash."), ["UTILITY SummonerFlash"]);
});

test("words Whisper glues or splits", () => {
  assert.deepEqual(timers("Mid-Flash"), ["MIDDLE SummonerFlash"]);
  assert.deepEqual(timers("Midno ignite."), ["MIDDLE SummonerDot"]);
  assert.deepEqual(timers("a d c no flesh"), ["BOTTOM SummonerFlash"]);
});

test("a spell only one enemy has needs no role", () => {
  assert.deepEqual(timers("smite down"), ["JUNGLE SummonerSmite"]);
  assert.deepEqual(timers("Vodno heal."), ["BOTTOM SummonerHeal"]);
  assert.equal(parseVoice(words("flash"), GAME).problem, "Whose Flash?");
});

test("spells the enemy doesn't have are rejected", () => {
  const result = parseVoice(words("ADC no ignite"), GAME);
  assert.deepEqual(result.commands, []);
  assert.equal(result.problem, "Jinx has no Ignite");
});

test("several commands in one hold", () => {
  assert.deepEqual(timers("ADC no flash no heal, support no flash"), [
    "BOTTOM SummonerFlash",
    "BOTTOM SummonerHeal",
    "UTILITY SummonerFlash",
  ]);
  assert.deepEqual(timers("cleanse ADC", []), ["BOTTOM SummonerBoost"]);
});

test("undo, and nothing useful", () => {
  assert.deepEqual(timers("undo"), ["undo"]);
  assert.equal(parseVoice(words("Thank you."), GAME).problem, "Didn't catch a command");
  assert.equal(parseVoice(words("Lee Sin, Thresh."), GAME).problem, "Which spell?");
});

test("timers count from the first word of the command", () => {
  const heard = [
    { text: "ok", agoMs: 4000 },
    { text: "ADC", agoMs: 2500 },
    { text: "no", agoMs: 2300 },
    { text: "flash", agoMs: 2100 },
  ];
  assert.deepEqual(parseVoice(heard, GAME).commands, [
    { kind: "timer", role: "BOTTOM", spell: "SummonerFlash", agoMs: 2500 },
  ]);
});

test("outside a game every role/spell pair is allowed", () => {
  assert.deepEqual(timers("supp ignite", []), ["UTILITY SummonerDot"]);
  assert.deepEqual(timers("top no ghost", []), ["TOP SummonerHaste"]);
});

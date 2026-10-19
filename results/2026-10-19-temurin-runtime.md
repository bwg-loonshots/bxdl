# 선정 Temurin JRE 준비·서명 Mac 패키지 검증

2026-10-19 · BXDL `feat/engine-rc1-adoption` 미커밋 변경.
[설계](../design/2026-10-19-jre-selection.md) · [사용법](../docs/runtime.md) · [정제 JSON](./2026-10-19-temurin-runtime.json).

## 결과

Eclipse Temurin21.0.12.1+1-LTS full JRE를 Mac arm64 후보로 고정하고 Rust `runtime stage`를 구현했다.
공식 archive/API/checksum을 대조한 JRE Home259파일 전체를 새 staging으로 복사·검증했다.
RC1 JAR를 재빌드하지 않고 새 CLI와 묶은 외부 Ed25519 서명 package에서 실제 설치·초기화·기동·종료·재시작을 통과했다.
이전 Oracle21.0.7 시험은 과거 조합의 기록으로 보존한다.

| 검사 | 결과 |
| --- | --- |
| make check | fmt·all-target Clippy 경고0, 290 PASS / 0 failed / 0 ignored |
| Mac release / Linux 타입 검사 | PASS / PASS. Linux 실행 시험 아님 |
| runtime stage | 259파일,157,631,121bytes, 링크0, 원본 legal 포함. 원문 lock/provenance 일치 |
| package build/verify | 외부 Ed25519 key,270파일,324,979,068bytes, verified-external-ed25519 |
| RC1 identity와 cold5사례 | MATCHED; OPEN/ALLOWLISTED 정상 입력은 INCOMPLETE, 누락/충돌/중복 issuer 거절, data 미생성 |
| 실제 TTY setup | --from 없는 전체14문답·등록 모드 안내·취소/재개·apply/등록 PASS |
| init/start | fresh RocksDB 초기화·LaunchAgent/engine RUNNING·로컬 READY |
| stop/동일 DB restart/stop | 정상 종료 확인, 새 attempt/nodeInstanceId·동일 genesis, 재시작 READY, 두 번째 정상 종료 |
| 최종 재개 | 추가 엔진 attempt 없음; service NOT_LOADED,engine STOPPED,busy=false |
| 독립 검토 | staging 경로·링크·변경 탐지·provenance·BOM 대조, 발견 결함 없음 |

290개는 library254,CLI13,CLI integration2,setup CLI5,setup install11,setup safety5다.
추가 runtime10개는 변조·누락·추가·위험 mode·링크 탈출/순환/디렉터리·lock 중복·입력변경·overwrite 거절 및 정상 복사를 확인한다.
시험 TLS는 선정 Temurin의 keytool로 새30일 PKCS12 key/truststore를 생성해 유효기간을 확인했다.
기존 시험 NGVK는 test-owned 복사본을 사용하며 기존 DB/설정/키는 변경하지 않았다.
consensus auto-start/sync OFF·peer 없는 단일 validator의 local lifecycle이며 **전역 합의는 NOT_CHECKED**다.

## 정확한 조합

| 항목 | 값 |
| --- | --- |
| NIGO RC1 source | 540a89f0814c01d2a4ec67da1087af35195882ca |
| NIGO JAR SHA256 | f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45 |
| JRE archive SHA256 | dec50fc6f9fcd4fe3ae8cabf5a5fa68f6afc48841f7698e468e9aa5d54beed84 |
| runtime lock SHA256 | fb779d849cdb9153b694b3ad368dc64b178af71a118d148dcd4d51a3e7853761 |
| Java launcher SHA256 | 9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c |
| CLI SHA256 / bytes | 4116961ec89be9f54091db79d6f0142e4c04bc3de541d839b3f3a1a0dd8139c8 / 2,708,416 |
| BXDL 구현 source SHA256 | d3be0a9916a1c456662fa6502c27eb66bdc96a7904b004d3aba2d8dfc3cb628c |
| 제품 archive SHA256 | 06f6c6c42ccfe296c449061282266f15e9ce210884f2580477ae93fcd0b6a362 |
| 제품 manifest SHA256 | 67bbb6a73457c3451d933347a868eb1278c6a508f2114f4d721d06f31373c95c |
| 시험 환경 | macOS26.6.2(25G83),arm64,Temurin21.0.12.1+1-LTS,RocksDB,fresh data |

source digest는 Cargo.toml·Cargo.lock·src/**/*.rs의58파일을 상대 POSIX 경로 문자열순으로 정렬하고
`path UTF8 + NUL + bytes + NUL`을 누적한 SHA256이다. 제품 revision은 development다.
실제 모듈49개와 원본 Home/제외 bundle 파일·공급 SBOM 차이는 [관측 snapshot](../contracts/runtime/temurin-21.0.12.1-macos-arm64/observed-runtime.json)에 둔다.
완성 package·서명 key·PKI·DB·미정제 로그·다운로드 JRE/source archive는 Git에 넣지 않는다.

## 남은 검증과 NIGO 요청

- 공급자 SBOM JRE hash 불일치의 원인은 미확정이다. 원문 보존·직접 생성한 runtime-only BOM과 실제 package inventory를 구분한다.
- 공식 TLS 다운로드/API/checksum은 대조했지만 공급자 detached signature는 검증하지 않았다. byte hash 대조를 서명 검증으로 표시하지 않는다.
- 대응 소스와 build scripts·metadata를 로컬 확보했다. 실제 고객 source 제공 경로와 전체 제품 NOTICE/SBOM은 후속이다.
- NIGO 전용 worktree의 REQ-0003-bxdl-jre-selection.md에 실행 조건·재빌드 없는 exact-JAR harness·같은 JRE의 crypto/mTLS/QBFT 검증을 요청했다.
  J1~J3는 회신/실행 대기이며 BXDL이 대신 PASS로 바꾸지 않는다. 문서·코드 commit/PR 공유는 아직 하지 않았다.
- 4-validator·전체 G1-M·최소 지원 OS·Apple notarization·Linux/systemd/Docker·기존 DB upgrade/downgrade는 미검증이다.

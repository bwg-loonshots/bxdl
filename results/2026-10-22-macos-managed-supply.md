# V2 공급 자료 보관·시작 진단 Mac 검증 — 2026-10-22

**새 RC.2 시험 package의 지정 로컬 lifecycle 13사례 PASS.** 등록 후 Documents에 있던 원본 archive·외부 공개키·engine lock 디렉터리를 옮겨 세 원본 경로가 모두 존재하지 않는 상태로 검증했다. 원본 bytes는 별도 보존했다. setup 재개·check·cold 검사·명시 init/start·진단·정상 stop·같은 DB 재시작이 성공했다.

[정제 기계 결과](./2026-10-22-macos-managed-supply.json), [설계](../design/2026-10-22-managed-supply-and-start-diagnostics.md)를 따른다. 이전 RC.2 A/B 결과와 하네스는 수정하지 않았다. 이번 PASS는 과거 파일 열기 대기의 특정 파일/TCC 원인을 규명하거나 기존 INCOMPLETE를 변경하는 판정이 아니다.

## 정확한 시험 조합

| 항목 | 값 |
| --- | --- |
| BXDL 기반 commit | `13e2aeef93e38767109beb1005c073435b72117a` + 이번 미커밋 구현 |
| manifest 제품 revision | `development` — clean commit 빌드로 표시하지 않음 |
| CLI SHA-256 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| archive SHA-256 / bytes | `cdd01d823ec3cd3843b63640c81b83f39bb79798a9fb55b664e05d8af21491e8` / 207,155,895 |
| manifest SHA-256 | `c439069a9974bcac5e947960db3c99434cbc52ef08838bd4ccb9311c2dde67c0` |
| payload | 270파일 / 325,295,952 bytes |
| 엔진 | RC.2 source `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b` |
| JAR SHA-256 | `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10` |
| JRE | Temurin21.0.12.1+1-LTS full JRE, macOS arm64 |
| Java SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

새 외부 Ed25519 시험 key로 서명한 package를 새 경로에 설치했다. setup/native/product/control/data도 분리했다. 자격 자료는 이전 private RC.2 시험 자료의 복사본이며 신규 생성이라고 주장하지 않는다. 합의 자동 시작과 동기화를 끈 단일 노드 로컬 profile이다. 고객 키·DB·설치는 변경하지 않았다.

## 확인 결과

- 등록 V2의 `trust/` 세 파일은 검증된 원본과 SHA가 같고 디렉터리0700·파일0600이었다.
- 원본 경로 부재 뒤 setup `--resume`/`check`와 cold 검사가 통과했다. cold 결과는 `configurationBinding=MATCHED`, **INCOMPLETE**를 유지했다.
- 설치 도우미에서 별도로 승인한 새 DB init과 LaunchAgent start가 성공했다. 같은 attempt에서 로컬 READY와 `startupStage=ENGINE_EXEC`를 확인했다.
- `diagnose`에도 정제된 시작 단계가 포함됐다. 두 시작 journal 각각에 7단계 진입 기록이 순서대로 존재했다. 단계 기록만으로 작업 완료를 추론하지 않았으며 실제 READY·정상 STOPPED는 별도 상태 근거로 확인했다.
- 정상 stop2회와 같은 DB의 새 attempt 재시작이 성공했다. 자동 재시작·강제 종료·재초기화를 사용하지 않았다.
- 별도 종료 검사에서 기록된15 PID 잔존0, 두 LaunchAgent NOT_LOADED, 시험2포트 TCP LISTEN0을 확인했다. bind 성공/실패를 종료 판정으로 사용하지 않았다.
- 최종 설치 payload270파일의 정확한 membership·hash·size·mode가 일치했다. 원본 보존본·관리 보관본·운용 입력도 불변이며 실행 CLI와 설치 CLI hash가 같다.

Rust **363개**, Python **34개**, fmt·clippy·Mac release build와 Linux 대상 타입 검사가 PASS다. V1 호환, 보관본 변조·링크·권한·부분 복사, plan pin 집합, 단계 투영과 정상 단계 진행 중 관측/쓰기 충돌의 회귀를 포함한다. Linux 실행 시험은 수행하지 않았다.

실제 실행의 raw 결과·PTY·JSON·하네스·독립 종료 검사28파일을 별도 private inventory로 고정했다. 빌드에 사용한 Rust source/Cargo64파일도 SHA를 대조해 보존했다. 해시는 정제 JSON에 기록했다. archive·개인키·자격 자료·DB·미정제 출력은 Git에 넣지 않는다.

## 남은 범위

V1 등록의 자동 이동/변환, 운용 native·제품·chain·키 경로의 macOS 접근 권한 검사, 로그아웃·sleep·전원 차단, 고객 upgrade, 4-validator quorum과 전체 G1-M은 이번 시험 범위가 아니다. V2에서도 운용 참조와 설치/control/data는 계속 필요하다.

[RC.3 수신](../contracts/nigo/engine-rc3-2026-10-22/README.md)은 별도로 PASS이며 이번 실제 package는 RC.2다. RC.3 lock/spec 명시 선택·새 package 인수·A7 offline Storage UX는 미수행이다. RC.3 공급자 검사 결과를 BXDL 제품 인수로 승계하지 않는다.

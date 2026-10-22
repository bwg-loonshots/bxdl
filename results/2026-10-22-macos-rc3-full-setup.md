# RC.3 전체 설치 문답·취소·재개 검증 — 2026-10-22

**PASS — 같은 RC.3 서명 archive의 전체14문답·취소/EOF·대기 중 HUP·설치/로컬 운용21사례와 독립 검사8항목.**
[정제 JSON](./2026-10-22-macos-rc3-full-setup.json), [앞선4-validator 결과](./2026-10-22-macos-rc3-package.md),
[앞선 제품 설정 import·원본 부재13사례](./2026-10-22-macos-rc3-setup.md)를 구분한다.

[NIGO PR #230](https://github.com/bwg-loonshots/nigo-protocol/pull/230)의 정식 RC.3 인수 회신을 먼저 머지한 뒤 실행했다.
merge는 `cd3e3e08fca915934d5a8af5da0af518059f57d2`, 회신 commit은 `f3f94e14cb996c5d7f027598aa64aa30a2c9ee5d`다.
[당시 회신](https://github.com/bwg-loonshots/nigo-protocol/blob/cd3e3e08fca915934d5a8af5da0af518059f57d2/requirements/REQ-0003-bxdl-rc3-receipt.md)의
전체 문답·취소/EOF/HUP `NOT_RUN`과 앞선 RC.3 결과의 미실행 표시는 그 시점의 사실로 보존한다.

## 실행 조합

| 항목 | 값 |
| --- | --- |
| 환경 | macOS26.6.2 (25G83), arm64, 사용자 GUI 세션·LaunchAgent |
| 실행 BXDL checkout | `9eb097aeba435979a473949b3085bb945fa9bac1` |
| Rust 제품 source | `53bb11b90d9d51aba5a7b509074046fd6ee78401`과65파일 동일 |
| CLI SHA-256 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO RC.3 source | `7ec5abd8c98e1cf070763a16f815e9281b74a5c1` |
| JAR SHA-256 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| runtime | 선정 Temurin21.0.12.1+1-LTS full JRE,259파일·49모듈 |
| archive SHA-256 / 크기 | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` / 207,160,872 bytes |
| manifest SHA-256 | `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123` |
| payload / 서명 | 270파일·325,299,323 bytes / 외부 Ed25519 시험 key |

제품 코드·CLI·JAR·JRE·archive를 변경하거나 재빌드·재서명하지 않았다. 앞선 RC.3 실행의 시험 자격 자료를 복사하고
새 DB·설치/control/setup 세션을 사용했다. 네트워크 도우미 PTY로 native 설정을 만든 뒤 로컬 profile을 명시적으로
고정했다. consensus auto-start=false·sync=false·peer0이며 이 실행의 `globalConsensus=NOT_CHECKED`다.
앞선4-validator 합의를 재실행하거나 기존 RC.2 DB를 upgrade한 시험이 아니다.

실행 전 전용 하네스3파일·독립 감사 도구·checkout과 NIGO 회신 revision을 hash로 고정했다. 제품 설정을 가져오지 않고
실제 PTY의14개 질문에 답했다. 한 번의 실행에서 각 거절·취소·재개 순서를 검증했으며 전체 실패를 숨기는 재시도는 없었다.

## 실제 UX 결과

| 흐름 | 확인한 결과 |
| --- | --- |
| 공급 문답 중 취소 | exit5, 설치/control/data 출력 없음 |
| 도움말·잘못된 값·뒤로·EOF | 입력 검증·기존 답 수정·checkpoint 보존, EOF exit5 |
| 초안 대기 중 HUP | controlling PTY 종료/SIGHUP 뒤 저장한 답으로 재개 |
| 전체14문답·계획·수정 | 모든 답과 생성 제품 설정 일치; 수정 후 기존 계획 무효화·명시 재계획 |
| apply 거절·확인 중 EOF/취소 | 출력과 초기화·서비스 생성 없이 종료/재개 |
| 명시 apply·V2 등록 | 같은 서명 package 설치·등록, cold MATCHED/INCOMPLETE 보존 |
| init 거절·finish | 확인 없이 초기화하지 않음 |
| 원본 공급 경로 부재 | V2 archive·공개키·lock 보관본 검증 후 원본 세 경로를 옮겨 보존; 부재 상태로 후속 운용 |
| 명시 init | 새 DB 초기화1회·genesis 확인 |
| start 거절·finish | 확인 없이 서비스를 시작하지 않음 |
| 명시 start·wait | 같은 attempt의 로컬 READY 확인 |
| 실행 중 메뉴 대기 HUP | setup controller 종료 뒤 서비스 PID·attempt·nodeInstance 유지 |
| 실행 중 재개·check·finish | 기존 attempt를 관측하며 새 init/start 생성 없음 |
| 정상 stop·같은 DB 재시작 | 정상 stop2회, 같은 data identity·genesis와 새 attempt로 READY |
| 정지 후 최종 재개 | 새 작업 없이 정지 상태 관측 |

원 결과21사례와 시간 순서별 관측21건이 PASS다. controller 종료 기록12개는 정상 exit0 6개·명시 취소/EOF exit5 4개·
의도한 대기 중 HUP 2개다. 독립 검사는 초안 checkpoint의 답3/5/14개·계획 revision1/2, 확인 전 mutation 부재,
재개 시 추가 작업 부재, 초기화 후 data identity 유지와 실제 init1회·service2회·stop2회를 다시 대조했다.

**HUP은 문답·메뉴 대기 중 controlling PTY 종료 범위**다. apply/init mutation 도중 crash·Mac 로그아웃·터미널 앱 전체 종료·
전원 차단을 검증한 것으로 확대하지 않는다. 자동 재시작·재초기화나 강제 노드 종료는 사용하지 않았다.

## 독립 검사와 진단 경계

독립 감사8항목은 원 결과 계약, package inventory, 공급 자료/binding, lifecycle, diagnostics, source identity,
시간 순서별 UX, OS cleanup이다. 같은 실행의 저장 증거와 실제 종료 상태를 읽기 전용으로 대조했으며 노드를 재실행하지 않았다.

- 설치 payload270파일·JRE259파일의 membership/hash/size/mode, V2 trust3파일·원본 보존본과 고정 입력17개를 재대조했다.
- 두 시작 attempt 모두7단계 진입 순서를 남겼고 live status/setup wait에서 READY를 확인했다.
- 오프라인 diagnose는 `startupStage=ENGINE_EXEC`, **`runtimeReadiness=NOT_OBSERVED`**다. 저장된 마지막 단계 표시와
  별도 live READY 관측을 구분하며 진단 파일 생성 성공을 새 readiness 증거로 바꾸지 않는다.
- 기록된28 PID 잔존0·두 LaunchAgent NOT_LOADED·시험2포트 TCP LISTEN0을 확인했다. bind 가능 여부를 판정에 쓰지 않았고
  독립 감사에서 추가 stop/bootout/cleanup mutation을 수행하지 않았다. 미해결 controller와 mutation은 없다.

제품 코드와 package가 그대로여서 이 근거 기록 작업에서 `make check`를 재실행하지 않았다.
Rust363개·Python50개·fmt/Clippy·Mac release build와 원격 CI는 [BXDL PR #14](https://github.com/bwg-loonshots/bxdl/pull/14) 및
[앞선 package 결과](./2026-10-22-macos-rc3-package.md)의 이력이다. 이를 이번에 다시 실행한 검사로 표시하지 않는다.

## 증거 보존과 남은 범위

원 결과 SHA-256은 `ffe6dc87ef9720d854ce767ce9cac36eca46e372ba0001f262b8c9f49cfe8dc5`, 독립 감사는
`9c6ab18c3332c347eab2bf088caba6a5ebf72ba6f8f029095459e5c2bc1180a8`다. 원자료133파일·412,583 bytes의 inventory와
보존본141파일·486,191 bytes의 membership/hash/size/mode를 대조했다. 보존 inventory SHA-256은
`319cb674969861208dbd292b80b985cce619c3ee007c45baccb3f80a8b54322a`다. JSON은 허용 필드만 선택해 작성했으며
절대 경로·PID 값·job label·attempt/node ID·원문 설정/자격 자료·키·PTY/로그·DB·archive를 Git에 넣지 않는다.

기존 RC.2 A/B의 원본 INCOMPLETE와 경로/TCC 대기 원인 미확정, RC.3 수신/앞선 실행/NIGO 회신의 당시 NOT_RUN은 그대로다.
이번 성공으로 모든 운용 자료 경로의 접근 문제가 해결됐다고 하지 않는다. V1 등록은 자동 변환하지 않으며 운용 설정·chain·키 참조는 계속 필요하다.

A7 offline Storage·기존 설치 upgrade·안전 제거·mutation 중단·로그인/로그아웃·sleep/wake·전원 차단·전체 G1-M,
J5 공급망/고객 재배포·최소 OS·Linux/systemd·Docker·multi-host·장기/성능은 별도다. NIGO의 과거 비용 원인 UNRESOLVED와
REQ-0002/0003 OPEN·development/officialRelease=false·계약 PROPOSED를 승격하지 않는다.

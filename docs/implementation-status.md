# 구현 상태 — 2026-10-22

Rust·macOS Apple Silicon 우선 방향을 유지한다. R1 설정 초안 이후 **R2의 Mac 새 폴더 패키지 설치**와 **R3의 NIGO 개발 후보 정보·cold 사전검사**를 추가했다. clean NIGO 후보를 별도로 수신하고 제품 설정과 명시 native QBFT 설정을 연결하는 preflight를 추가했다. 이후 설치본·설정을 등록해 `--instance`로 preflight와 명시 init/resume-init을 호출하고, Mac 사용자 LaunchAgent의 start/status/stop을 연결했다. 이번에는 `setup --install`에서 이 단계를 선택하고 재개하는 A~C 흐름을 구현했다. 기존 package의 단일 validator 서비스 근거와 새 setup의 exact package·TTY 인수, 전체 G1-M을 구분한다.

## 현재 방향 — console 개발 중단, 직접 CLI 우선

2026-10-22 사용자가 실제 console의 직관성이 부족하다고 평가해 개발 중단을 요청했다. 기존 console 코드·시안·검증 이력은 보존하고 새 메뉴·화면·서비스 연결은 진행하지 않는다. 공통 catalog/home/target와 Preparation 검증·저장 API는 직접 CLI에서 재사용한다.

**다음 작업은 CLI-01: console 없이 기존 노드에 이름을 붙이고 목록을 조회하는 명령**이다. 이후 이름 기반 운용(CLI-02) → 순차 설치 준비/재개(CLI-03) → 실제 설치 실행(CLI-04) → 자동완성·새 package·사용자 과업(CLI-05) 순서다. [최신 설계](../design/2026-10-22-cli-simplification.md)와 [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다.

현재 `setup new/resume`도 7개 번호 메뉴를 사용한다. 이를 아직 CLI 우선 UX로 개선했다고 주장하지 않으며 CLI-03에서 필요한 값만 순차 질문하는 흐름으로 교체한다. `nodes add`, 이름 기반 운용, 선형 문답은 계획 상태다. 아래 UX-01~03의 PASS는 당시 구현의 기술 검증이며 이번 방향 변경만으로 새 실행 기능이나 사용성 검증이 추가되지는 않았다.

## 통합 UX 3단계 — 이름 기반 CLI와 설치 자료 준비

`bxdl setup new <이름>` / `bxdl setup resume <이름>`과 console 홈 1번을 같은 준비 controller에 연결했다. 생성 승인, 제품 가져오기·개별 수정, 공급 자료·공개 network의 자기 노드 선택 또는 기존 native, 정책별 중간 저장, 검토 승인 뒤 immutable 제품/native revision 저장을 제공한다. 이전 답과 출력은 보존하고 변경한 입력의 준비 결과만 무효화한다. 입력 변경·동일 bytes 교체·경로 충돌·저장 충돌을 검사하며 package/control/data는 생성하지 않는다.

`make check` **Rust464·Python50·fmt·Clippy**, Linux 타입 검사와 실제 Mac PTY의 생성 거절/승인·직접/console 재개·미완성 수정·EOF 보존을 통과했다. [사용 가이드](./preparation.md)와 [UX-03 검증 결과](../results/2026-10-22-ux03-preparation.md)를 따른다. 이후 실제 설치·등록·init 연결은 CLI-04, `nodes`와 이름 기반 `start/status/stop/logs`는 CLI-01/02에서 진행한다. [CLI 간소화 설계](../design/2026-10-22-cli-simplification.md)에 명령·이름·호환 계약을 반영했다. 기존 명령·JSON·저장 schema와 NIGO RC.3/JRE pin은 유지한다. 새 package 실인수·사람 과업은 후속이다.

## 통합 UX 2단계 — 홈·목록·기존 자료 읽기

`bxdl console [--home <관리-루트>] [--instance <control>]`을 구현했다. 홈은 무쓰기이며 등록 V1/V2·설치 세션·제품 초안·네트워크 준비 세션을 직접 지정해 읽는다. 별칭을 붙인 목록 저장·갱신·숨기기만 y/yes 뒤 수행하고, 저장 단계와 현재 서비스 미조회를 구분한다. 잘못된 관리 root, 대상 inode 교체, 목록 손상, 오래된 목록의 동시 갱신, private 입력/DB/package를 침범하는 저장 위치를 거절한다. 목록 실패가 기존 등록/설치 결과를 바꾸지 않는다.

새 관리 root marker와 catalog는 별도 schema V1이다. 기존 setup/workflow/instance 저장 형식과 개별 CLI JSON/exit는 유지한다. 목록에는 저장한 공개 단계만 보관한다. console의 실제 설치·서비스 연결은 중단하고, 직접 CLI 연결은 CLI-02/04에서 진행한다. [console 가이드](./console.md)와 [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다. `make check`(Rust 427개·Python 50개·fmt·Clippy), Linux 타입 검사, 실제 Mac PTY의 홈 무쓰기·목록 저장 거절/승인·재열기/숨기기·원본 보존을 통과했다. [UX-02 검증 기록](../results/2026-10-22-ux02-console-catalog.md)을 따른다. 실제 엔진 등록 직후 목록 실패 주입·새 package 인수는 후속이다.

## 통합 UX 1단계 — 공통 문답·수정·안내

[통합 UX 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)의 UX-01을 구현했다. 기존 제품·네트워크·설치 도우미의 모든 질문·출력·확인에서 `:help`·`:back`·`:cancel`을 공통 처리하고, 번호로 항목 하나를 수정한 뒤 검토로 돌아간다. 공개 식별자·주소·포트는 검토하되 자격 자료의 원문 경로는 검토 목록에 표시하지 않는다. 변경 없는 답·잘못된 값·취소는 기존 설치 계획을 보존하고 유효한 실제 변경만 계획을 무효화한다.

명령별 `--help`와 공통 상태 안내를 추가했다. 알려진 cold `INCOMPLETE`, 관측 기한 만료, 실제 실패, 상태 불명, 정상 종료 확인 필요/완료를 구별하며 JSON envelope·exit code·저장 schema와 실행 승인 조건은 유지한다. `make check`(Rust 383개·Python 50개·fmt·Clippy)와 실제 PTY 초안 수정·출력 거절 검증을 통과했다. [UX-01 검증 기록](../results/2026-10-22-ux01-common-prompts.md)을 따른다.

통합 홈의 기존 구현 이력은 위 UX-02/03을 따르며 현재 추가 개발은 중단했다. NIGO/engine lock/JRE·기존 accepted archive는 바꾸지 않는다. 변경 CLI를 기존 package에 덮어쓰지 않으며 새 archive의 실제 서비스·네트워크 인수는 CLI-05에서 별도로 수행한다.

## 등록 자료 보관과 시작 진단 — 2026-10-22

[새 설계](../design/2026-10-22-managed-supply-and-start-diagnostics.md)에 따라 새 등록 V2는 검증한 archive·engine lock·선택 외부 공개키를 control의 private `trust/`에 보관한다. 등록 이후 worker와 setup 재개는 원본 경로를 다시 읽지 않고 보관본을 검증한다. 원래 경로/hash는 출처로 남기고 설치 inventory·서명·엔진 pin 대조는 유지한다. 기존 V1은 자동 변환하지 않으며 native·제품·chain·키 참조와 설치/control/data는 계속 필요하다.

시작 journal의 `startupStage`와 허용된 `startupFailureReason`를 status/setup/diagnose에 표시한다. 마지막 진입 단계이며 완료 증거나 특정 파일/TCC 원인의 확정이 아니다. timeout/UNKNOWN을 자동 성공·재시작으로 바꾸지 않는다. 인수 helper는 bind 가능 여부와 실제 OS LISTEN 관측을 분리하도록 보완한다.

**새 RC.2 시험 package의 지정 로컬 lifecycle 13사례가 PASS**다. 원본 공급 세 경로 부재 상태의 재개·init/start/stop·진단·동일 DB 재시작을 확인했다. 독립 검사에서 기록된15 PID 잔존0·두 job NOT_LOADED·시험2포트 LISTEN0·설치 inventory270파일 일치를 확인했다. Rust363개·Python34개·fmt/clippy·Mac release build·Linux 타입 검사도 PASS다. [새 검증 기록](../results/2026-10-22-macos-managed-supply.md)을 따른다. RC.2 Run A/B와 기존 package PASS/INCOMPLETE는 당시 bytes의 이력으로 보존한다.

## 현재 선택한 RC.3 — 지정 Mac 패키지 재인수 PASS

[RC.3 snapshot](../contracts/nigo/engine-rc3-2026-10-22/README.md)은 source `7ec5abd8…`·JAR `05a38bd1…`의 별도 수신 기록이다. 원격 archive22파일/21checksum, 원본 manifest·계약과 고정 source, Console fingerprint/assets를 대조했고 선정 Temurin의 실제 `engine-info` 전체 identity가 일치했다. JRE259파일·49모듈을 확인하고 실행 후 공급22파일의 불변을 검사했다.

수신 검증은 **PASS**이며 당시에는 제품 후보를 선택하거나 노드·init·Storage inspect/repair·서비스를 실행하지 않았다. snapshot의 미선택·package/A7 NOT_RUN은 그 시점의 기록으로 보존한다.

이후 사용자 요청으로 RC.3를 명시 선택하고 [전용 engine lock](../packaging/engine-lock.rc3-temurin-development.json)·[package spec](../packaging/package-spec.rc3-temurin.example.json)을 추가했다. 제품 source는 PR #13의 `53bb11b9`이며 선정 Temurin full JRE를 유지한다. [재인수 설계](../design/2026-10-22-rc3-package-reacceptance.md)에 따른 같은 새 서명 archive의 **4-validator·두 정족수 복구 경로·wrong-pin과 원본 공급 경로 부재 상태의 setup 13사례가 PASS**다.

[4-validator 결과](../results/2026-10-22-macos-rc3-package.md)는 거래8개·제출9회, 독립된 두45초 정족수 상실 후 실시간 무재제출 복구와 같은 네 DB 전체 재시작·명시1회 재제출 복구를 포함한다. 정상 stop12회와 독립 검사에서 기록된62 PID 잔존0·12개 job NOT_LOADED·8포트 LISTEN0을 확인했다. 5개 설치본의 각270파일과 V2 보관 자료도 대조했다.

[같은 archive의 setup 결과](../results/2026-10-22-macos-rc3-setup.md)는 원본 archive·lock·공개키 세 경로가 없는 상태의 재개·init/start·진단·정상 stop·같은 DB 재시작 13사례다. 정상 stop2회와 기록된14 PID 잔존0·2개 job NOT_LOADED·2포트 LISTEN0을 확인했다. 이13사례 시험은 제품 설정을 가져온 뒤 공급 자료 문답을 진행했으며 당시 전체14문답·cancel/hangup 경로는 재실행하지 않았다. Rust363개·Python50개·fmt·Clippy·Mac release build가 PASS다.

RC.2 결과와 공급자의 RC.3 Storage/QBFT 검증은 각 이력으로 보존한다. **offline Storage A7은 별도 구현·인수 대상이며 NOT_RUN**이다. 기존 설치 upgrade와 전체 G1-M도 후속이다.

## RC.3 전체 설치 UX 후속 — 2026-10-22

[NIGO PR #230](https://github.com/bwg-loonshots/nigo-protocol/pull/230) merge `cd3e3e08`으로 RC.3 지정 인수를 정식 회신한 뒤 같은 archive·CLI·JAR·JRE를 재빌드 없이 시험했다. **전체14문답·수정·취소/EOF·대기 중 HUP·원본 공급 경로 부재 상태의 설치/운용21사례와 독립 검사8항목이 PASS**다. [새 전체 UX 결과](../results/2026-10-22-macos-rc3-full-setup.md)를 따른다.

초기화1회·서비스2회·정상 stop2회, 확인 전 mutation과 재개 시 추가 작업 부재를 대조했다. 기록된28 PID 잔존0·두 job NOT_LOADED·두 시험 포트 LISTEN0과 설치270파일·JRE259파일·고정 입력17개·V2 보관본3파일 일치를 확인했다. HUP은 문답/메뉴 대기 중 PTY 종료이며 mutation crash·Mac 로그아웃은 미시험이다. diagnose의 ENGINE_EXEC/NOT_OBSERVED와 live READY 관측을 구분한다.

제품 source는 기반 `53bb11b9`와65파일이 같고 CLI도 동일하다. 이 기록 작업에서 `make check`를 재실행하지 않았으며 Rust363/Python50·fmt/Clippy·Mac build 및 PR #14 CI는 앞선 이력이다. RC.3 앞선13사례·package 결과와 NIGO 회신의 당시 전체 문답 NOT_RUN, RC.2 원본 INCOMPLETE·경로 원인 미확정은 보존한다. A7·upgrade·안전 제거·전체 G1-M·J5는 후속이다.

## 이전 엔진 선택과 Mac 패키지 인수 이력 — 2026-10-21 RC.2

NIGO `nigo-engine-v0.1.0-rc.2`(source `67c9f8e5`, JAR `b1aa84cb…`)를 명시 선택했다.
[수신 snapshot](../contracts/nigo/engine-rc2-2026-10-21/README.md)과 RC.2 전용 lock/spec을 추가하고,
RC.1 자료·기존 INCOMPLETE 결과는 이력으로 보존한다. 계약·Console·DB/schema·선정 JRE는 동일하며 Rust 제품 source 변경은 없다.

새 서명 Mac package의 설치·등록·명시 init·네트워크 설정 PTY·LaunchAgent start/status/stop 경로에서
4/4·3/4 거래, 동일 DB catch-up, 2/4 45초 후 재제출 없는 복구, 별도 skew 형성 후 같은 네 DB 전체 재시작·명시1회 재제출·후속 거래, wrong-pin 격리를 **PASS**했다.
8개 거래의 receipt·블록 포함·동일 hash/root를 독립 대조했고 정상 stop12회·기록된62 PID 잔존0·시험8포트 listener0을 확인했다.
[실제 결과와 범위](../results/2026-10-21-macos-rc2-package.md), [설계](../design/2026-10-21-rc2-package-reacceptance.md)를 따른다.

Rust343개와 Python28개 회귀·Mac release build가 통과했다. 후보 혼용·재제출·복구 판정 검사는 `make check`에 포함한다.
이 package 인수 당시 RC.2 전체 setup 설치 문답은 별도였으며 아래 후속 실행으로 구분한다. 기존 RC.1 설치의 RC.2 upgrade, 전체 G1-M·J5·고객 배포는 미완료다.
아래 RC.1·이전 후보 기록의 PASS/INCOMPLETE는 당시 입력과 범위로 유지한다.

## RC.2 전체 설치 UX 후속 — 2026-10-21

[NIGO PR #225](https://github.com/bwg-loonshots/nigo-protocol/pull/225)에서 A4 수신/선택과 지정 package 복구 인수를 정식 회신한 뒤 같은 archive·CLI/JRE/JAR로 실제 TTY 전체 설치 흐름을 검증했다. 제품 source나 package는 바꾸지 않았다.

[새 결과](../results/2026-10-21-macos-rc2-setup.md)의 run B는 14문답·수정·취소/EOF·실제 터미널 hangup·재개, apply/init/start의 거절/승인, 같은 attempt의 READY, 정상 stop2회·동일 DB 재시작 등 **20개 사례 PASS**다. 다만 정리 시 포트 bind 실패로 원본 자동 최종 집계는 **INCOMPLETE**이며 그대로 보존한다. 후속 독립 검사에서 설치 inventory270파일·입력15개가 일치하고 기록된26 PID·두 LaunchAgent·두 포트의 listener가 없음을 확인했다.

앞선 run A는 Documents의 원본 archive/key/lock 참조를 사용했고 초기화 이후 start worker의 파일 열기 대기·UNKNOWN으로 INCOMPLETE였다. 명시 종료 요청 뒤 worker가 취소로 종료했고 잔존 등록만 정리했다. TCC를 포함한 원인은 미확정이며 B 통과로 해결됐다고 판단하지 않는다. 위 후속 변경에서 공급 자료 보관과 마지막 시작 단계·실제 LISTEN 관측을 보완했으며 새 package의 지정 로컬 시험은 별도 기록했다. 이 진단은 모든 경로의 접근 권한 사전 검사를 보장하지 않는다. 이 두 실행은 단일 노드 로컬 lifecycle이며 이전 4-validator 근거와 구분한다. 전체 G1-M은 아직 완료하지 않았다.

## 이전 엔진 후보 — 2026-10-19 RC1

NIGO `nigo-engine-v0.1.0-rc.1`(source `540a89f0`, JAR `f1ea80e2…`)을 원격 수신·검증하고
[계약 snapshot](../contracts/nigo/engine-rc1-2026-10-19/README.md), 새 lock/spec 예시를 추가했다.
등록 모드 OPEN/ALLOWLISTED와 빈 허용목록의 의미를 설치 계획에 표시하며 기존 journal 형식을 유지한다.
이후의 9월 구현/시험 기록은 당시 303e 후보의 이력이다.

`make check` 280개·Mac release 빌드·Linux 타입 검사 PASS. 실제 RC1과 외부 Ed25519 시험 key로 서명한 package에서
전체 14문답·취소/재개·cold/등록/init/start/stop·동일 DB 재시작을 통과했다. 최종 service NOT_LOADED·engine STOPPED다.
4-validator 합의는 NOT_CHECKED이며 test-only JRE를 정식 재배포 대상으로 선정하지 않았다.
[전체 근거와 남은 범위](../results/2026-10-19-engine-rc1-adoption.md)를 따른다.

## 선정 JRE — 2026-10-19 Temurin

Temurin21.0.12.1+1 전체 JRE·archive/source/launcher/259파일을 고정하고 제작자용 `runtime stage`를 추가했다.
사용자 설치 마법사에는 JRE 선택 단계를 추가하지 않는다. 290개 자동 검사·Mac release·Linux 타입 검사를 통과했고,
선정 JRE+RC1의 실제 서명 package에서 전체14문답·취소/재개·설치/init/start/stop·동일 DB 재시작이 PASS다.
최종 노드는 정지했으며 4-validator 합의는 NOT_CHECKED다. 당시 NIGO 공급자 호환성·전체 제품 공급망·출시는 후속으로 남겼다. 아래 최신 회신 수신 상태를 함께 따른다.
[새 검증 기록](../results/2026-10-19-temurin-runtime.md)을 따르며 이전280개/Oracle 기록은 당시 조합의 이력이다.

## 최종 잠금 수정과 재검증

설치 세션 잠금 해제를 보완하고 회귀 2개를 추가해 자동 검사 292개를 통과했다.
최종 패키지의 설치·초기화·첫 기동/종료는 통과했지만 재시작 15초 관측은 UNKNOWN으로 끝나 자동 하네스는 INCOMPLETE다.
같은 실행의 후속 READY·정상 종료·재개를 별도로 확인했다. [최종 기록](../results/2026-10-19-workflow-lock-release.md)을 따른다.

## NIGO JRE 회신과 시작 관측 UX

NIGO PR #212의 J1~J3를 검토해 현재 RC1·Temurin full JRE 개발 후보 조합에서 수용했다.
공급자 exact-JAR 하네스 9/9 PASS와 runtime 259파일·49모듈 일치를 확인했으며
[수신 기록](../contracts/nigo/jre-feedback-2026-10-19/README.md)에 검토 revision·근거 hash·범위를 고정했다.
공급자의 storage-open→RUNNING 시간은 HTTP READY·합의 완료·기동 SLA가 아니며,
이전 BXDL 15초 관측 INCOMPLETE 판정은 그대로 유지한다.

`status --wait`와 설치 도우미 `wait`로 같은 시작 시도의 준비 상태를 이어서 관측한다.
관측 기한 만료와 실제 실패를 구분하고 시도 ID·마지막 관측 사유를 보존한다.
설치 화면에서 확인한 시도가 바뀌면 대기를 중단하며 자동 재시작하지 않는다.
[설계](../design/2026-10-19-start-observation-ux.md)를 따른다. 자동 검사312개·Mac release·Linux 타입 검사를 통과했고, 실제 signed package에서
1초 status 관측 만료→같은 시도의 setup/CLI READY→정상 종료·재개를 확인했다.
[최종 검증 기록](../results/2026-10-19-start-observation-ux.md)의 범위와 예비 시험을 구분한다.

## 네트워크 설정 준비

별도 `setup --network`에서 기존 제품 설정·승인 chain·전체 validator 공개 목록을 받아 RC1 native `node.json`을 준비한다.
공개 인증서의 NIGO MTLS pin 계산, local/peer identity·endpoint·권한·pin/membership 정적 검사,
TLS 저장소 형식과 새 네트워크/참가·합의·archive 정책 선택, 취소/재개·입력 고정·새0600 파일 게시를 구현했다.
기존 제품 v1·설치 journal은 유지하며 기존 NGVK/TLS/password 참조를 재사용한다.
[설계](../design/2026-10-19-network-preparation.md)와 [사용 가이드](./network.md)를 따른다.
키 발급·observer·trusted checkpoint·실제 4-validator 거래/인증 인수는 포함하지 않는다.
자동 검사343개·Mac release·Linux 타입 검사를 통과했고, 실제 TTY 취소/재개 및 new-network/join-network의 RC1 cold INCOMPLETE·binding MATCHED를 확인했다.
[검증 기록](../results/2026-10-19-network-preparation.md)을 따른다. peer3개는 합성 공개 입력이며 실제 네트워크 합의·새 package 설치는 이번 시험에 포함하지 않았다.

## 동일 package의 4-validator 실행 — 2026-10-20

네트워크 준비 UX를 `fecf7a6`으로 커밋하고 같은 RC1·Temurin 서명 archive로 실제 네 validator를 설치·기동했다.
4/4·3/4 거래 확정, 같은 DB의 canonical catch-up과 재합류 후 새 거래, 2/4 상태 45초 동안의 no-finality를 확인했다.
독립 새 네트워크에서는 잘못된 expected pin의 outbound WRONG_PIN·peer 0과 정상 세 노드의 TLS1.3·거래 확정도 통과했다.

**전체 결과는 INCOMPLETE**다. 2/4→3/4 복구 뒤 원 거래가 90초 내 확정되지 않았고,
같은 DB의 네 노드 재시작·명시 재제출에서도 모든 노드가 거래를 보유하고 연결됐지만 round가 수렴하지 않았다.
당시 NIGO의 라운드 복구 검토와 고정 후보 재인수가 필요했다. 현재 RC.2 결과는 위 별도 기록을 따르며 원래 실패 판정은 보존한다. 당시 모든 시험 노드를 정상 종료했다.
[실행별 근거·NIGO 확인 사항](../results/2026-10-20-macos-qbft-package.md)과 [재현 도구](../tests/acceptance/README.md)를 따른다.
아래 9월/10월19일 단일 노드·NOT_CHECKED 기록은 각 당시 시험 범위이며 이번 결과로 덮어쓰지 않는다.

## 실제 제공 기능

- `runtime stage`: 신뢰 lock의 전체 JRE 파일을 검증해 새 staging·provenance 생성. 오프라인 제작자용이며 다운로드/JVM 실행 없음.
- `package build/verify`: 기대 hash·결정적 tar/gzip·외부 Ed25519 key·전체 파일 inventory·경로/형식/한도 검사.
- `install`: Mac arm64 개발 package를 새 명시 경로에 설치. 기존 대상 거부, 검증 중 파일 비실행, 최종 gzip 검사 뒤 receipt 기록. 중단 폴더를 자동 덮어쓰기·삭제·resume하지 않음.
- `engine inspect`: 신뢰 lock의 정확한 JAR/Java hash 및 engine-info build/source/console/contract identity 대조.
- `engine preflight`: 명시 NIGO native config/chain을 입력받는 cold JVM 검사. timeout/출력 한도·환경 분리, engine exit 3을 BXDL INCOMPLETE/exit 5로 보존.
- `config validate`/기본 `preflight`: BXDL 제품 설정·로컬 metadata 검사. 엔진 옵션이 없는 preflight는 엔진을 실행하지 않는다.
- 결합 `preflight`: 제품/local 검사, 명시 QBFT VALIDATOR/MTLS 설정과 ID·경로·endpoint·키 참조 일치, 같은 native snapshot의 pinned engine cold 검사. 제품/설정/chain hash와 결과를 연결한다. 로컬 FAIL·불일치는 JVM 실행 전에 거부한다.
- `config fingerprint` / `setup --network`: 공개 leaf 인증서 pin 계산·공통 validator/peer 목록에서 RC.1~RC.3 공통 native 계약의 설정 준비. 엔진·네트워크 검증과 구분한다.
- `setup`: 14개 제품 설정 입력·수정·checkpoint·재개·기존 JSON 가져오기·새 JSON 출력. 파일 저장 성공은 engine/service 준비 완료가 아니다.
- `setup --install --workspace`: Mac arm64 TTY에서 archive·신뢰 key·lock·native 입력과 제품 설정을 계획으로 고정하고, apply/init/start를 각각 확인한 뒤 기존 API로 연결한다. 별도 세션 잠금·소유 증거·완료 결과를 확인해 명시 재개한다. 기본 초안 모드는 유지한다.
- `instance register/show`: archive·외부 key·engine lock·전체 설치 manifest·제품/native/cold 결과를 고정해 private 등록 journal 생성, 저장 상태와 advisory busy 조회. 새 V2 등록은 archive/lock/선택 공개키 보관본을 사용하고 V1은 원본 참조를 유지한다. 운용 설정·chain·키 참조는 계속 필요하다.
- `preflight --instance`: 등록 입력 hash·전체 설치본을 재검증하고 기존 결합 cold 실행. 정상도 INCOMPLETE다.
- `init/resume-init`: 작업별 확인 flag, durable intent, inherited advisory lock, 단발 NIGO 호출, process exit/stdout/동일 attempt report/engine journal 대조. 불명 결과와 시도 자료를 보존한다.
- `start/status/stop`: Mac 사용자 GUI 세션의 시도별 LaunchAgent, 일회 gate와 같은 PID의 Java exec, report·launchd·로컬 HTTP 관측, 정상 종료 증명 후 등록 정리. `status --wait`와 설치 도우미 `wait`는 같은 시도의 READY·실패·종료를 읽기 관측하고 시간 초과를 구분한다. `startupStage`는 마지막 진입 검사이며 완료로 추정하지 않는다. 로그인 자동 시작·자동 재시작은 제공하지 않는다.

`upgrade/uninstall`은 미구현이다. `logs/diagnose`는 아래 오프라인 범위로 제공한다. 제품 config에서 NIGO QBFT peer/pin/validator 상세 설정을 추측 렌더링하지 않는다. `setup`의 instance.json과 `engine preflight`의 node.json을 구별한다.

## 새로 수신한 NIGO 결과

문서 HEAD `49d1cefc`(#119)에서 clean 인계 자료를 읽었다. source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false인 실제 JAR `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`를 별도로 검증했다. [새 원문·공급자 증거 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)과 [소비자 실행 결과](../results/2026-09-18-clean-engine-preflight.md)를 따른다. 기존 dirty 후보 snapshot·결과는 당시 이력으로 보존했다.

#118의 QBFT drain·fresh sync·genesis-only observer·실제 source 종료 후 재선택 보완과 clean JAR 공급은 제공자 근거에서 확인됐다. BXDL에서는 같은 clean JAR를 담은 시험 Mac package를 새로 설치하고 **한 validator의 실제 QBFT cold 설정·NGVK/TLS 자료 검사**까지 확인했다. 이전 cold 검증은 peer 없는 정적 fixture였으며 당시 실제 4-node 운영·DB/native·launchd·G1-M은 수행하지 않았다. NIGO 저장소·요구 원장은 변경하지 않았고 REQ-0002 OPEN/계약 PROPOSED를 유지한다.

## 이전 등록·초기화 구현의 근거와 한계

새 경로의 실제 테스트·실행·중단 주입 결과는 [인스턴스 초기화 검증 기록](../results/2026-09-18-instance-initialization.md)에 별도로 기록한다. 위 이전 cold 결과를 DB 초기화·runtime·전체 G1-M 증거로 소급하지 않는다. 공급 source는 같은 clean `303e163a`이며 새로운 NIGO 계약/원장 변경은 없다.

초기화의 exit 0은 저장소 초기화와 종료 확인이다. launchd 등록·노드 시작·네트워크 정상·4-validator 인수가 아니다. timeout은 TERM 후 최대 5초 대기하며 SIGKILL하지 않아 child와 잠금이 남을 수 있다. UNKNOWN 뒤 자동 재시도·INITIALIZED 자동 채택은 없다. V1 원본 또는 V2 관리 보관본의 archive/key와 credential hash 고정, 매 작업의 전체 설치본 재검증을 유지한다.

제어 기록은 같은 UID의 로컬 파일이며 암호학적 상태 로그가 아니다. inode에 고정한 control 복사/이동·복원 절차와 수동 결과 채택은 미구현이다. 각 시도의 약 160 MB JAR snapshot·report는 남고 자동 GC는 없다. 세부 사용법은 [인스턴스 가이드](./instance.md), 상태 전이와 실패 정책은 [설계](../design/2026-09-18-instance-initialization.md)를 따른다.

등록·초기화 구현은 `make check` 162개 테스트, Mac release 빌드, Linux 타입 검사를 통과했다. 설치된 당시 Mac package의 실제 NIGO JAR로 초기화, 실제 controller crash에서의 잠금 유지·UNKNOWN 보존, 합성 중단 checkpoint에서의 실제 resume을 확인했다. [검증 기록](../results/2026-09-18-instance-initialization.md)에 시험 범위와 JRE 모듈 보완을 구분했다. PR #4의 Mac·Ubuntu fast CI 통과도 그 revision의 이력이며 이번 LaunchAgent 변경의 근거로 소급하지 않는다.

## 이전 LaunchAgent 구현의 근거와 한계

서비스 명령과 내부 worker gate를 구현했다. macOS 26.6.2 arm64의 실제 새 package에서 새 install/register/init 후 start/status·중복 start 거부·정상 stop을 확인했다. 같은 DB 재시작에서는 nodeInstanceId가 바뀌고 genesis가 유지됐다. start controller만 강제 종료한 시험에서도 LaunchAgent worker/Java가 계속 진행해 로컬 READY에 도달했고, 중복 start는 거부한 뒤 정상 stop했다. 소비된 attempt의 직접 kickstart 재실행 거부·데이터 hash 불변과 정상 종료 뒤 job 등록 해제도 확인했다. 이는 peer 없는 단일 validator의 서비스 시험이며 거래 확정·4-validator 인수가 아니다.

당시 `make check`의 fmt·Clippy·200개 테스트와 `make check-linux` 타입 검사는 통과했다. 이번 설치 도우미의 검사 결과로 전용하지 않는다. 원격 CI 결과는 해당 PR의 revision별 검사 기록을 따른다. 시험 경로·제약·실제 결과는 [LaunchAgent 검증 기록](../results/2026-09-18-macos-launchagent.md)을 따른다. Documents 아래 첫 시험은 worker 진입 전 exit 78로 종료했고 임시 경로로 입력을 분리한 시험은 동작했으나, OS 접근 제어/TCC를 원인으로 확정하지 않는다. 설계와 실패 정책은 [LaunchAgent 문서](../design/2026-09-18-macos-launchagent.md), 실제 사용법은 [인스턴스 가이드](./instance.md)를 따른다.

start에는 설치 package의 `bin/bxdl`과 동일한 바이트의 CLI가 필요하다. 기존 초기화용 package에 서비스 CLI만 덮어쓰는 방법은 사용할 수 없다. 새 archive·설치본의 전체 inventory를 검증하며 기존 등록/data의 migration은 후속이다. 시도별 worker/JAR snapshot과 private 로그는 보존하고 자동 용량 정리는 제공하지 않는다.

정상 STOPPED report·launchd의 프로세스 부재·control 잠금 획득을 함께 확인한 뒤에만 STOPPED_VERIFIED를 게시한다. status는 읽기 전용이며 복구 기록을 쓰지 않는다. gate 실패·UNKNOWN은 자동 init/repair/restart로 해결하지 않는다. stop timeout에 SIGKILL·live job bootout을 하지 않아 엔진과 잠금이 남을 수 있다. start timeout은 cold 명령(최대120초)과 사전 검증·bootstrap 뒤 시작 대기에 각각 적용되며, 실행 취소나 전체 명령 총 시간 제한이 아니다.

현재 profile은 수동 bootstrap한 사용자 LaunchAgent다. 자동 로그인 시작, 장기 운영 LaunchDaemon, 로그아웃/OS 종료의 escalation·sleep/wake 인수, 같은 package의 4-validator 거래/재시작과 전체 G1-M, 선정 JRE의 정식 공급망·최소 macOS 인수는 남아 있다. 같은 최종 package로 `Library/Application Support/BXDL` 아래 고유 시험 control/data/config에서도 register/init/start/status/stop을 확인했다. 공백 경로를 포함한 이 호스트의 결과이며 추가 OS·접근 정책 조합을 보장하지 않는다.

## 오프라인 logs/diagnose

`logs --instance`는 최신 초기화·서비스 시도의 고정 이벤트와 저장된 상태를 읽는다. `diagnose --instance --output`은 같은 정제 결과를 새0600 JSON으로 저장한다. 원문 stdout/stderr·설정·credential·ID·경로·hash는 내보내지 않는다. 서비스가 꺼져 있어도 동작하며 JVM·네트워크·launchctl·DB를 실행하거나 복구하지 않는다. 수집 누락·손상·변경·크기/시간 제한은 partial과 사유로 남긴다. 파일 생성 성공은 노드 health나 초기화의 새 검증이 아니다.

설계와 한도는 [오프라인 진단 설계](../design/2026-09-21-offline-diagnostics.md), 실제 검사 근거는 [진단 검증 기록](../results/2026-09-21-offline-diagnostics.md)을 따른다. raw/follow·전체 시도 검색·로그 rotation·보존/용량 정리·UNKNOWN 조정·안전한 제거는 후속이다. NIGO 공급 revision과 요구 원장은 그대로 유지한다.

## setup 설치 흐름 A~C

`--install --workspace <새 세션>`과 `--resume`은 기본 초안 모드와 독립된 진입점이다. `--from`으로 제품 JSON을 가져올 수 있으며 설치 모드는 `--json`, `--output`, `--non-interactive` 혼용과 비터미널 입력을 거부한다. 공급 자료·현재 CLI 일치 검사, 계획 고정, 새 제품 설정 출력·파일 설치·cold·등록, 별도 명시 init/resume-init/start를 연결했다.

세션 container 아래 workflow·draft·generated를 분리하고 세션 전체 잠금과 inode 소유 기록을 유지한다. installer/register의 create-new 직후 기록한 소유 증거와 전체 완료 결과가 함께 맞는 경우에만 재개에서 완료를 재구성한다. partial·외부 결과는 자동 채택하지 않으며 엔진 UNKNOWN을 자동 성공이나 새 초기화로 전환하지 않는다.

최초 범위는 workspace와 package/control/data의 **기존 부모**를 요구한다. package/control/data 기본 제안은 workspace의 sibling이다. 계획했던 Application Support 상위 폴더 자동0700 준비는 미구현으로 남겼다. 자동 GC 값과 발행자 주소 수는 안내만 하며 native/chain 설정을 수정하지 않는다. 기존 clean `303e163a` 공급 pin과 검증 이력을 유지하고 이번에 최신 NIGO JAR로 교체하지 않는다.

`make check` 270개 테스트와 Mac release 빌드·Linux 타입 검사를 통과했다. 새 CLI를 담은 unsigned-development package와 실제 TTY에서 취소·재개, 공백 경로 설치·등록, 별도 init/start, 정상 stop과 동일 DB 재시작을 확인했다. 단일 노드 로컬 READY이며 전역 합의는 NOT_CHECKED다. 최종 상태는 job NOT_LOADED·engine STOPPED다. 실제 강제 중단·실패 주입과 전체 G1-M은 남았다. [검증 기록](../results/2026-09-27-setup-install-flow.md), [설계와 남은 인수](../design/2026-09-27-setup-install-flow.md), [사용법](./setup.md)을 따른다.

## 계획 대비 상태

| 작업 | 상태 | 남은 조건 |
| --- | --- | --- |
| BX-001/005 | IN_PROGRESS | 첫 Mac profile, 최소 OS/JRE/native·launchd 인수 |
| BX-002/010 | DONE | Rust 기반·CLI envelope·고정 toolchain/lock |
| BX-003 | IN_PROGRESS | Temurin21.0.12.1+1 full JRE 고정·runtime stage·runtime BOM/source 확보와 NIGO J1~J3 수용. 전체 제품 NOTICE/SBOM/소스 전달·출시 인수 후속 |
| BX-004 | IN_PROGRESS | RC.1 이력·RC.2 지정 package PASS 보존, BXDL PR #11/#12·NIGO PR #225/#229 공유. RC.3 전용 lock/spec 선택 후 동일 Mac archive의 4-validator·두 복구·wrong-pin·setup13 및 독립 정리 PASS. A7 NOT_RUN·전체 제품 인수 후속 |
| BX-011~014 | IN_PROGRESS | build/verify/새 폴더 설치 구현, 정식 공급·릴리스/업데이트 후속 |
| BX-020 | IN_PROGRESS | v1 제품/native 대조와 validator network/peer/pin 입력·공통 native 렌더 구현. RC.2 이력 보존, 새 RC.3 archive의 4-validator 거래/catch-up·두 정족수 복구 경로·pin 거부 PASS. observer 등 확장 후속 |
| BX-021/022/024 | IN_PROGRESS | instance journal·명시 init/resume·gated run과 기존 정상 종료/같은 DB 재시작 근거 유지. 새 V2 등록 자료 보관·시작 단계 진단의 RC.2 로컬13사례와 새 RC.3 동일 archive의 setup13사례 PASS. 전체 실패·복구/운영 인수 후속 |
| BX-023 | IN_PROGRESS | 제품/native cold·실제 RocksDB 단발 초기화 확인. 지속 runtime·DB/WAL 복구·service 인수 후속 |
| BX-030 | IN_PROGRESS | RC.2 지정 경로20사례 PASS·원본 INCOMPLETE·독립 정리 PASS와 자료 경로 문제 OPEN 보존. RC.3 동일 archive의 setup13사례와 후속 전체14문답·취소/EOF·대기 중 HUP·원본 부재 운용21사례 PASS. mutation 중단·전체 인수 후속 |
| BX-031/032 | IN_PROGRESS | 수동 LaunchAgent·status·정상 stop·restart와 같은 시도 관측 대기 구현. 로그인/로그아웃·sleep/wake와 전체 사용자 lifecycle 인수 후속 |
| BX-033 | IN_PROGRESS | 오프라인 최신 시도 logs/diagnose·정제·bounded partial 출력 구현. 전체 시도 조회·보존/용량 정책 후속 |
| BX-034 | PLAN | 데이터·키를 보존하는 안전한 제거 |
| BX-040~043 | IN_PROGRESS | RC.2 이력 보존, 새 RC.3 동일 archive의 4-validator 거래/catch-up·두 복구·pin 거부·setup13사례 PASS. G1-M 전체 사용자 흐름·Linux/Docker 후속 |
| BX-044 | IN_PROGRESS | LaunchAgent PR #5 fast CI 통과. 진단 변경의 로컬 검사는 연결된 결과 기록을 따르며 원격 CI는 해당 PR 기록 참조. 전체 package 인수·릴리스 pipeline 후속 |
| BX-050~053/M6 | PLAN | 버전 쌍 업데이트·offline 유지보수·고객 운영 확대 |

## 다음 순서

우선 [CLI-01~05](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)의 직접 CLI 개선을 진행한다. console의 후속 화면·서비스 연결은 중단했다. 아래는 그와 별도로 유지하는 전체 제품의 후속 과제다.

NIGO PR #230 정식 회신 뒤 완료한 RC.3 전체 설치 UX21사례·독립 검사8항목의 근거는 [전체 UX 결과](../results/2026-10-22-macos-rc3-full-setup.md)에 기록했다. 기존 회신과 앞선 결과의 당시 NOT_RUN, RC.2 원본 INCOMPLETE·경로 원인 미확정은 보존한다.

1. Mac 중단·실패 경계·로그인/로그아웃·sleep/wake와 데이터·키를 보존하는 안전한 제거를 구현·검증한다. 제거(REM-01)는 G1-M 완료 전 필수 항목이다. 저장 기록과 실제 health를 구분하며 UNKNOWN 뒤 자동 재시작하지 않는다. 운용 참조 경로의 사전 접근 검사도 남아 있으며 기존 설치의 binding 변환/upgrade·DB migration은 별도다.
2. J5의 upstream SBOM archive hash 불일치·서명 출처 확인·전체 제품 NOTICE/SBOM·대응 소스 전달·최소 macOS를 확정한다. NIGO J1~J3와 RC.2 공급·복구 회신은 수용했으며, 이 실행 호환성 근거를 공급망 승인으로 전용하지 않는다.
3. RC.3의 offline Storage A7을 별도 구현·인수한다. 서비스 중지·자동 재기동 차단·backend/경로 매핑·백업·명시 repair 확인·UNKNOWN·재시작을 구현하고 검증해야 한다. 수신 PASS와 이번 package 재인수는 A7 완료를 뜻하지 않는다.
4. 검증한 바이트의 정식 릴리스 pipeline과 나머지 공통·Mac 인수를 완료해 G1-M을 확정한다. 이후 버전 쌍 업데이트·지원 진단/보존 정책을 확장한다. Linux/systemd G1-L, Docker G1-D는 Mac과 분리해 검증한다.

R2 파일 receipt는 instance 등록·NIGO journal이 아니며 전체 R2/R3 완료라고 표시하지 않는다. 현재 Mac 사용자 profile의 CLI·엔진은 같은 UID다. 파일 권한만으로 강한 격리를 보장하지 않는다. 기존 사용자 node/data/key는 시험하지 않는다.

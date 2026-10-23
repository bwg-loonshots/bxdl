# 독립 관리 CLI와 기존 RC.3 — 2026-10-23

## 판정·범위

**지정 Mac 조합 PASS.** 기존 RC.3 archive를 재조립·재서명하지 않고 새로운 관리 CLI 두 빌드로 설치·등록·초기화·이름 기반 운용·정상 종료·같은 data 경로 재시작을 확인했다. 직접 명령 **16개 사례**, 기존 `setup --install`의 실제 PTY **8개 사례**가 통과했다. [정제 결과 JSON](./2026-10-23-independent-management-cli.json)에 identity·사례·실패 이력을 기록한다.

실제 시험은 macOS arm64, 한 validator, `consensus.auto-start=false`, sync 비활성화, peers 없는 격리 fixture다. 로컬 READY 이유는 `MANUAL_START_INITIALIZED`다. Java/LaunchAgent와 관리 CLI 교체 호환성을 확인했으며, 합의·거래 확정 인수를 다시 수행한 결과는 아니다.

## 고정 identity

| 대상 | SHA-256 |
| --- | --- |
| 기존 RC.3 archive | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| 기존 manifest | `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123` |
| 기존 실행기와 모든 worker | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| 관리 CLI A3 | `d502fa43ebf2c5821a193d928a31e75d6cef24f777623d83cac7824091916dad` |
| 관리 CLI B3 | `394b5276722a003db66905e25448f1847ecc1ac0100d79d4627f4c03e0752df1` |
| NIGO JAR | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| Java | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

지원 profile은 `BXDL_RC3_EXECUTOR_V1`이다. A3/B3는 같은 최종 제품 source에 `BXDL_REVISION=independent-cli-a3-20261023` / `independent-cli-b3-20261023`을 각각 적용한 빌드다. 둘 다 package의 기존 실행기와 다른 bytes이며, 바이너리 교체 후에도 같은 catalog/control/data를 사용했다. 서로 다른 미래 구현의 호환을 포괄적으로 인증한 결과는 아니다.

빌드 기반은 `0854dd94bada644a7e8ac9082f23ab3fd92181e5`와 이번 미커밋 worktree 변경이다. `src/**/*.rs`와 Cargo.toml/Cargo.lock/rust-toolchain.toml **84개 파일**의 정렬된 path/hash JSON에 대한 SHA-256은 `5948db615fa4d6a9ece603980342a2459144a7af065d9434ba763cb291a5fbb7`이다. 빌드 이후 제품 source 불변을 확인했다. 정확한 계산 방식은 JSON에 기록한다. 기존 패키지 source는 종전 `53bb11b9…`이며 NIGO/JRE lock은 바꾸지 않았다.

## 자동 검사·응답 계약

- 최종 `make check`: **Rust 512개, Python 50개, fmt·Clippy PASS**.
- 실행기 adapter 17개와 관리 CLI 통합 12개 회귀 포함. 인증 실패·미지원 실행 방지·대상 교체·명령 옵션·오프라인 진단 호환·입력 미소비·제한된 출력/시간·UNKNOWN 보존을 검사했다.
- 보존한 실제 RC.3 응답 **45개**를 최종 parser로 읽었다: init 5, start 13, stop 14, status 9, preflight 1, show 2, diagnose 1. 원본 private 경로에 의존하는 임시 검사는 제거하고 정제한 nullable READY/진단·genesis 회귀를 영구 테스트로 남겼다.
- `make check-linux` PASS는 타입 검사이며 Linux 서비스 실행 인수가 아니다.
- adapter·workflow·이름 선택·응답 파서를 별도 검토했다. 기존 manifest/engine의 엄격한 JSON 검증은 완화하지 않았다.

## 직접 운용 — 16개 사례

A3가 기존 signed archive를 검증·설치하고 V2 등록·이름 연결·cold preflight·init을 수행했다. B3가 동일 관리 목록을 읽었고, 공급 archive/lock/공개키의 원래 복사 경로를 이동한 뒤에도 control의 검증된 보관본으로 운용했다.

A3의 `start <이름>` → B3의 동일 attempt 조회·logs/diagnose·명시 경로 status·정상 stop → B3의 같은 data 경로 재시작 → A3의 정상 stop을 통과했다. 두 start의 worker가 모두 기존 실행기 hash였다. data 재사용은 동일 설정 경로와 `engine-instance.json` bytes 보존 및 초기화 재실행 부재로 확인했으며, 디렉터리 inode 전후 기록은 수집하지 않았다.

독립 대조에서 두 attempt의 `STOPPED_VERIFIED`와 엔진 정상 종료 근거, 기록 PID **22개 모두 부재**, LaunchAgent **2개 NOT_LOADED**, 시험 포트 TCP LISTEN **0**을 확인했다. 설치 payload **270개 / 325,299,323 bytes**, 그중 JRE **259개**의 구성원·크기·mode·hash와 공급 자료·원본 입력이 일치했다.

## 기존 설치 도우미 — 실제 PTY 8개 사례

A3의 `setup --install`에서 plan/apply를 명시 승인해 같은 RC.3를 설치·등록한 뒤 종료했다. 원자료 경로 이동 후 B3가 동일 workflow를 resume하여 명시 init/start/wait와 READY를 확인했다. A3의 실행 중 resume/check가 B3의 같은 노드를 관측했고, B3가 정상 stop했다.

새로운 순차 문답이나 `setup apply <이름>`은 이번 구현이 아니다. 이 시험은 제품 설정을 가져온 뒤 기존 설치 도우미의 실행·재개를 검증했으며 전체 14문답·EOF/HUP 경로를 다시 실행하지 않았다.

package 270개·JRE 259개·worker 1개·공급 자료의 불변, 기록 PID **8개 부재**, LaunchAgent **1개 NOT_LOADED**, 시험 포트 TCP LISTEN **0**을 확인했다. 최종 독립 대조도 PASS이며, 전체 감사 기록의 SHA-256은 `1dc808e2ec5ccc4b8240f3cfb57b944f1597593528ccdc2a8c8a0bf5a87183b1`이다.

## 발견·수정한 문제와 이전 기록 보존

- **직접 A: INCOMPLETE.** 실제 init은 완료됐지만 adapter가 `0x`를 포함한 genesis hash를 잘못 거부해 `EXECUTOR_OPERATION_UNKNOWN`을 반환했다. init을 반복하지 않았고, 수정 CLI의 읽기 전용 show가 INITIALIZED를 확인했다. binding·journal·엔진 identity·첫 실패 결과는 그대로였다. 서비스 시도 0, 기록 PID 11개 부재를 확인했다.
- **직접 B: INCOMPLETE.** 시작 결과의 정상 `health.executionStatus: null`을 manifest용 JSON decoder가 거부했다. 노드는 명시적인 정상 stop으로 종료했고 기록 PID 14개 부재·job 부재·STOPPED_VERIFIED를 확인했다. executor 전용 decoder와 회귀를 추가했으며 실패 결과를 덮어쓰지 않았다.
- **첫 setup PTY: INCOMPLETE.** 하네스가 이전 신뢰방식 질문 문구를 기다려 중단됐다. 도우미는 PAUSED로 취소했고 package/control/data는 생성되지 않았다. 하네스만 수정해 새로운 격리 root에서 위 8개 사례를 실행했다.

최종 직접 C와 setup B는 각각 새 격리 설치이며 실패한 시도의 자동 init/start 재시도가 아니다. private 원본·하네스 snapshot·실패 및 정리 기록을 보존한다. 키·자격 자료·원문 로그·실행 파일·archive는 Git 결과에 포함하지 않는다.

## 남은 범위

지원하는 정확한 RC.3 조합에서 **관리 CLI만 교체 가능**하다. 실행기 자체·NIGO·JRE를 변경하면 새 runtime package와 조합 인수가 필요하다. 미지원 package의 자동 실행·기존 DB upgrade/migration·A7·로그아웃/sleep/crash·Linux/systemd·Docker·LaunchDaemon·전체 G1-M·정식 고객 배포는 이번 PASS에 포함하지 않는다.

NIGO 저장소의 변경이나 선행 구현은 없었다. 다음 제품 UX 단계는 [CLI-03 순차 설치 준비](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)다.

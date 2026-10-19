# 시작 관측 UX와 실제 Mac 패키지 검증

2026-10-19 · BXDL `e87a83d` 이후 변경 · [정제 JSON](./2026-10-19-start-observation-ux.json).
[NIGO J1~J3 회신](../contracts/nigo/jre-feedback-2026-10-19/README.md)을 수용하고 기존 RC1·Temurin full JRE를 유지했다.

## 구현과 자동 검사

`status --wait`와 설치 도우미 `wait`는 현재 시도를 고정해 관측한다. 관측 기한 종료, 실제 실패, 시도 변경을 구분하고
마지막 상태·사유를 보존한다. 조회 오류 뒤 이전 READY를 재사용하거나 자동으로 다시 시작하지 않는다.
HTTP·launchd의 실제 시간 초과는 identity 오류와 구분하며, start/stop에서 전파되는 timeout도 UNKNOWN/exit6를 유지한다.
기존 engine OperationLock과 journal·plan 형식/해시는 변경하지 않았다.

최종 `make check`는 **312 PASS / 0 failed / 0 ignored**이며 fmt·Clippy·Mac release·Linux 타입 검사를 통과했다.
Linux 타입 검사는 Linux 노드 실행 인수가 아니다. 최초 최종 검사에서는 release 빌드가 함께 실행되는 동안 기존 출력 제한 시험의
3초 경과시간 assertion 하나가 실패했다. 소스와 시간 기준을 변경하지 않고 단독 검사 및 전체 검사를 다시 실행해 통과했다.
독립 검토에서 지적한 I/O 만료 분류·시작 시간 안내·하네스 READY 판정을 보완했고 남은 조치 사항은 없었다.

## 실제 package의 결과

새 외부 Ed25519 시험 키로 서명한 package와 실제 TTY에서 다음을 확인했다.

1. RC1 등록 정책 cold 5사례, 전체14문답, 취소/재개, 설치·등록·초기화, 첫 READY·정상 종료.
2. 같은 DB에 `start --timeout-seconds 120`을 한 번 요청하고 새 시도를 관측했다.
3. 그 실행에 `status --wait --timeout-seconds 1`을 적용해 `SERVICE_STATUS_WAIT_EXPIRED`/UNKNOWN을 확인했다.
   당시 엔진 상태는 UNKNOWN, 마지막 사유는 SERVICE_REPORT_UNKNOWN이었고 시도 ID는 보존됐다. 실제 엔진 실패로 판정하지 않았다.
4. 설치 도우미와 CLI의 대기가 **같은 시도의 READY**를 확인했다. 화면의 정확한 준비 상태 행·시도 ID와 JSON을 각각 검사했다.
   백그라운드 start 명령의 최종 exit0·READY·동일 시도도 회수해 확인했다. 대기는 새 engine attempt를 생성하지 않았다.
5. 새 nodeInstanceId·동일 genesis를 확인하고 정상 종료했다. 마지막 도우미 재개도 새 시도를 만들지 않았다.

**이 범위의 자동 인수는 PASS**다. 최종 service NOT_LOADED·engine STOPPED·operationBusy=false다.
로컬 단일 validator 검증이며 전역 합의는 NOT_CHECKED다. 실제 시험은 status 대기 만료를 검증했고 start 대기 만료는 단위 검사 범위다.

## 예비 시험과 기존 기록

첫 예비 시험은 첫 기동/종료까지 통과했으나 `start --timeout-seconds 1`이 시작 전 cold 검사에서 ENGINE_TIMEOUT으로 끝났다.
새 재시작 attempt가 생성되지 않았고 최종 정지 상태를 확인했다. 이 하네스 결과는 **INCOMPLETE로 보존**하며 관측 만료 근거에 포함하지 않는다.
start의 시간 설정은 cold 명령(최대120초)과 bootstrap 뒤 준비 관측에 각각 적용된다. 전체 명령의 총 시간 한도가 아니다.
이후 정상 start와 짧은 status 관측을 분리한 새 시험을 별도 폴더에서 수행했고, 위 PASS는 새 시험의 결과다.

[기존 15초 재시작 시험](./2026-10-19-workflow-lock-release.md)의 INCOMPLETE 판정은 바꾸지 않았다.
새 시험은 엔진 기동 속도 개선이나 기동 SLA 확정의 근거가 아니다.

## 고정한 조합

| 항목 | 값 |
| --- | --- |
| BXDL source digest | c57d61de850c399a46ea80c143cf88f771f020e9bb2933126d4aab0756810f54 |
| CLI SHA256 | 2a25dc017b214a54f751b3a9dfe733a1930398f947511cecac43b8902da305c8 |
| package archive SHA256 | 00663c9b3ff42bbf88a533aede933b16e90ac69b4f96669c9a1fa81e289ba4d4 |
| package manifest SHA256 | 2de47e3cba3167616ec01da0da3d4ebbae18ee0a7d19d5eb8b688430209f3bc5 |
| package inventory | 270파일 / 325,012,332bytes |
| 환경 | macOS26.6.2(25G83), arm64, Temurin21.0.12.1+1-LTS, RocksDB |

source digest 방식과 하네스·원자료 hash는 정제 JSON에 기록했다. 원본 package·키·DB·미정제 로그는 Git에 넣지 않는다.
실제 package의 4-validator·전체 G1-M, source-free native/PKI 준비, J5 공급망·최소 OS·정식 출시는 후속이다.

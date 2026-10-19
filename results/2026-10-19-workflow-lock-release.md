# 설치 세션 잠금 해제 수정과 최종 패키지 재검증

2026-10-19 · BXDL `236b8f4` 이후 workflow lock 수정. [정제 JSON](./2026-10-19-workflow-lock-release.json).
[이전 Temurin 시험](./2026-10-19-temurin-runtime.md)의 identity와 PASS는 당시 구현의 이력으로 유지한다.

## 잠금 수정

[PR #8의 첫 Mac CI](https://github.com/bwg-loonshots/bxdl/actions/runs/39211415170/job/117925440660)에서
설치 세션을 닫은 직후 다시 열 때 `SETUP_WORKFLOW_BUSY`가 발생했다. 병렬 실행의 fork~exec 사이에
같은 열린 파일 설명을 상속한 자식이 남으면 마지막 close까지 flock이 유지될 수 있었다.
설치 도우미가 성공적으로 획득한 잠금만 guard로 감싸고 정상 종료와 생성 도중 오류에서 명시적으로 해제한다.
독립 자식에게 잠금을 넘기는 엔진 `OperationLock`은 변경하지 않는다.

상속된 설명이 남은 정상 종료·생성 오류와 다음 소유자의 배타성에 대한 회귀 2개를 추가했다.
`make check`는 292 PASS / 0 failed / 0 ignored, fmt·Clippy·Mac release·Linux 타입 검사도 통과했다.
Linux 타입 검사는 Linux 엔진 실행 인수를 뜻하지 않는다. 독립 코드 검토에서 추가 조치 사항은 없었다.

## 최종 실행 파일의 실제 결과

기존 RC1 JAR와 Temurin runtime을 유지하고 수정 CLI를 담은 새 외부 Ed25519 서명 package를 시험했다.
전체14문답·취소/재개·cold5사례·설치/등록·초기화·첫 기동/READY·첫 정상 종료는 통과했다.
동일 DB 재시작의 `start --timeout-seconds 15`는 `SERVICE_OPERATION_UNKNOWN`을 반환하여
**자동 하네스 전체 판정은 INCOMPLETE**다. 이후 결과로 원래 판정을 PASS로 바꾸지 않는다.

같은 재시작 attempt에서 `OPENING_MANAGED_STORAGE`(06:54:14.196Z) 이후
`RUNNING / LOCAL_STARTUP_COMPLETED`(06:54:33.071Z)를 기록했고, 별도 status에서 READY를 확인했다.
재시작 명령 반복·대기 시간 변경 없이 새 nodeInstanceId와 동일 genesis를 대조한 뒤 명시적으로 정상 종료했다.
마지막 설치 세션 재개는 엔진 attempt를 추가하지 않았다. 이 **후속 관측·종료·재개는 PASS**이며
최종 service NOT_LOADED, engine STOPPED, operationBusy=false, globalConsensus NOT_CHECKED다.
시작 지연의 원인과 선정 JRE의 호환성 판단은 NIGO 공급자 검증으로 남긴다.

빌드 완료 전에 시작해 이전 CLI를 담은 별도 준비 실행은 설치/등록/DB/엔진 실행 전 제외했다.
대기 중인 해당 시험 UI만 종료하고 원본 실패·제외 기록은 보존했다. 위 결과에 합산하지 않는다.

| 항목 | 값 |
| --- | --- |
| BXDL source digest | 5efb284de619a4f10f769cf1e72285e82bc605ae7070f6aab374c1f6618fe0c7 |
| CLI SHA256 | 4d97115ba682cc4409b52d84409bd8e1bcde12093742a20ed70027c9a75e1a70 |
| package archive SHA256 | 9c0542f8a98f60f89078bffa5cc1bca83d50b5009f547bdc2cf0de575121f84c |
| package manifest SHA256 | f127167baa016538587517c7f0f930025c9ca659a695ceae57b6c4e3948b4703 |
| package inventory | 270파일 / 324,979,068bytes |
| 환경 | macOS26.6.2(25G83),arm64,Temurin21.0.12.1+1-LTS,RocksDB |

source digest 방식은 정제 JSON에 기록했다. RC1/JRE/runtime lock 고정 값은 이전 Temurin 시험과 같다.
원본 시험 패키지·키·DB·미정제 로그는 Git에 넣지 않는다. NIGO J1~J3, 4-validator 합의·전체 G1-M·출시 인수는 대기 상태다.

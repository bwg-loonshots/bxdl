# 오프라인 logs/diagnose 설계

- 기준: BXDL main `8bb6ea4` 이후. NIGO clean `303e163a` 계약 유지.
- 범위: BX-033의 첫 로컬 운영 UX. NIGO 공유 저장소·요구 원장 변경 없음.

## 사용자 흐름

```sh
bxdl logs --instance <control-dir> --tail 50
bxdl diagnose --instance <control-dir> --output <new-report.json>
```

사용자는 노드가 정지했거나 서비스 시작에 실패한 때에도 최근 초기화·서비스 시도에서 남긴 사건과 누락 이유를 확인한다. diagnose는 같은 결과를 지원 전달용 새 JSON 파일로 저장한다. 설정·key·engine archive가 사라졌어도 보존된 control 기록을 읽을 수 있다. 새 설치·JVM·네트워크·launchctl·DB open은 필요 없다.

`--instance`는 등록한 control 폴더다. ID를 받아 임의의 경로를 검색하지 않는다. 현재 구현은 최신 초기화와 최신 서비스 시도만 읽고 전체 과거 시도·OS unified log·database를 순회하지 않는다. init의 stdout은 이전 구현에서 보존하지 않으므로 그 원문을 복원하지 않는다.

## 결과 의미

- `mode=OFFLINE_RECORDED_EVIDENCE`, `runtimeReadiness=NOT_OBSERVED`, `globalConsensus=NOT_CHECKED`.
- `initializationRecord`와 `serviceRecord`는 보존된 journal 상태다. `INITIALIZED`·`STOPPED_VERIFIED`는 과거에 기록된 판정이며 지금 DB가 정상이라는 새 증명이 아니다.
- `operationBusy`는 제어 잠금의 순간 관측이다. 잠금이 비었다고 Java가 종료됐다고 판단하지 않는다.
- events는 report.jsonl의 알려진 sequence/command/status/reason만 포함한다. `RUNNING`·`STOPPED`도 과거 엔진 보고다. 신뢰 build·현재 PID·DB·health와 재검증한 service status가 아니다.
- `partial`은 **수집 범위의 누락**이다. 미해결 초기화의 기록을 온전히 수집하면 partial=false일 수 있으며 exit 0도 노드 정상 의미가 아니다.
- 실패·미지의 형식·잘린 줄·source 변경·예산 제한은 partial과 고정 reason으로 남긴다. 정책상 원문 제외는 WITHHELD이며 그것만으로 partial은 아니다.

JSON CLI는 기존 envelope를 유지한다. 수집 완료 0, 부분 수집 5, 인자 오류 2, 파일 수집/출력 거부 3, stdout 쓰기 실패 7이다. partial diagnose도 안전한 등록 경계를 확인했으면 파일을 생성한다. 게시 이후 fsync 등의 실패는 `DIAGNOSTIC_EXPORT_UNCERTAIN`로 파일 존재 가능성을 알리며 재실행으로 덮어쓰지 않는다.

## 비밀정보 제외

임의 문자열을 정규식으로 부분 치환하고 안전하다고 선언하지 않는다. JSONL 전체의 duplicate key·depth·header sequence/PID/attempt/command/PROPOSED 조건과 알려진 event/detail 형식을 확인한 뒤 **컴파일된 상수**로 이벤트를 투영한다. timestamp·PID·instance/attempt/node ID·build/hash·details 값·경로는 출력하지 않는다. State.reason도 명시한 고정 집합 외에는 DETAIL_WITHHELD로 바꾼다.

`service.stdout.json`과 `service.stderr.private`는 본문을 읽지 않는다. 안전하게 확인한 크기와 RAW_TEXT_EXCLUDED_BY_POLICY만 담는다. 설정·credential·chain·DB·JAR·CLI snapshot은 읽거나 묶음에 복사하지 않는다. `--raw`와 `--follow`는 이번 UX에 없다. 같은 UID가 제어 기록을 바꿀 수 있는 신뢰 한계는 유지한다.

## 파일과 예산

1. control anchor의 기존 inode·잠금을 확인하며 누락 상태를 만들거나 수리하지 않는다.
2. binding은 private regular 0600/euid/nlink1로 읽는다. journal과 binding hash를 맞추고 최대128개 참조 경로의 문법을 확인한다.
3. journal reader는 읽기 **전에** 요청 byte cap을 적용한다. 최신 snapshot만 가져오며 재조회도 같은 예산에서 차감한다. Store 자체의 revision/entry 수 상한은 유지한다.
4. report는 attempt 아래 고정 파일만 최대32KiB 읽는다. symlink·hardlink·FIFO·느슨한 권한·소유자 불일치는 제외한다. 읽는 동안 변경된 파일도 제외한다.
5. `--max-bytes` 4096~1048576(기본262144)는 수집 payload 읽기와 최종 compact JSON 각각의 예산이다. 재검증 payload도 계산한다. control anchor·directory/inode 메타데이터 비용은 별도의 고정 상한이다. 오류 때는 시도한 read cap을 보수적으로 소진시킨다. stdout/stderr는 본문 byte를 소비하지 않는다.
6. `--tail` 1~200(기본50)은 초기화→서비스의 논리적 순서로 모은 이벤트의 마지막 N개다. 잘못된 wall clock을 사용해 전체 시간순으로 합치지 않는다. tail/최종 출력 cap으로 줄인 양과 이유를 기록한다.
7. `--timeout-seconds` 1~30(기본5)는 파일 연산 경계에서 확인한다. 마지막 관측 뒤에도 만료를 표시한다. 느리거나 멈춘 파일시스템 syscall을 강제로 중단하는 deadline은 아니다.

## 지원 파일 게시

등록 binding과 journal의 결속을 확인하고 수집 뒤 binding을 다시 읽어 변경을 배제한다. 손상·경계 불명인 등록은 logs partial만 가능하며 diagnose 출력은 거부한다. 예산이 큰 report에 소진돼도 마지막 binding 재확인용 예산을 먼저 남긴다.

output은 control/data/package/archive/제품·native 설정/신뢰 lock/key/모든 고정 참조와 분리해야 한다. 기존 inode와 Mac case/Unicode alias를 포함해 overlaps 검사한다. 부모는 이미 있어야 하고 자동 생성하지 않는다. component별 symlink·위험한 쓰기 권한을 거부하고 parent를 capability로 고정한다. 같은 폴더의 새0600 temp를 fsync한 후 exclusive link로 게시하고 부모를 sync한다. 기존 출력은 덮어쓰지 않는다.

## 검증·후속

비밀 canary, malformed/unknown/partial JSONL, 임의 journal reason, 부재·손상·동시 변경, 잠금 유지, bounded payload, 경로 alias·symlink/hardlink/FIFO, 기존 출력 불변을 회귀 검사한다. 정지한 실제 패키지 인스턴스의 과거 기록에도 실행한다. 결과는 [검증 기록](../results/2026-09-21-offline-diagnostics.md)을 따른다.

전체 과거 시도 검색·raw 로그 정제 확대·실시간 follow·로그 rotation·attempt/JAR/CLI snapshot 자동 정리·UNKNOWN 수동 조정은 후속이다. setup 연결, 세션 장애·4-validator·전체 오프라인 G1-M 인수는 이 기능의 완료와 별도로 진행한다.
